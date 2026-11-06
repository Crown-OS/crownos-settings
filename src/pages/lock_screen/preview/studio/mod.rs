//! The preview's painter: the lock screen's own layout and paint, and the
//! pictures behind it, made off the UI thread — loading the system's fonts
//! alone takes a noticeable while.
//!
//! Requests that pile up while it works collapse into the latest of each
//! kind, and the clock is repainted by itself whenever its face would change.
//! A layout paints only the sprites the canvas has no picture of yet. Glass
//! digits are filled here with the background they stand over, and filled
//! again whenever the canvas moves the clock. A depth cut runs in a helper
//! process the loop waits on beside its requests.

mod depth;
mod glass;
mod laid;
mod messages;
mod pictures;
mod thumbnails;

use std::future::pending;
use std::path::PathBuf;
use std::process::Child;
use std::time::Instant;

use crownos_config::schema::ClockFrame;
use lockscreen::clock::{ClockFace, frost_sigma, until_next_change};
use lockscreen::cosmic_text::FontSystem;
use lockscreen::layout::{Live, Rect, Sprite};
use lockscreen::paint::Painter;
use lockscreen::tiny_skia::Pixmap;
use lockscreen::user::User;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::{JoinHandle, spawn_blocking};

use crate::util::deadline::sleep_until;
use crate::util::worker::MessageProxy;
use glass::{GlassKey, frost_darkening};
use laid::{Laid, tallest_glass};
pub use messages::{
    BackdropRequest, DepthState, LayoutRequest, Painted, Picture, Slot, StudioCommand, StudioEvent,
};
pub use pictures::DESKTOP_ZOOM;
use pictures::{SlotKey, Source};
pub use thumbnails::{Shelf, ThumbnailRequest};

/// Dots in the sample password, enough to read as one.
const SAMPLE_PASSWORD_CHARS: usize = 6;

pub async fn run(proxy: MessageProxy<StudioEvent>, mut commands: UnboundedReceiver<StudioCommand>) {
    let mut studio = Studio::default();
    let mut next_face = None;
    let mut cut = None;
    loop {
        let mut work = Work::default();
        tokio::select! {
            command = commands.recv() => match command {
                Some(command) => work.add(command),
                None => return,
            },
            () = sleep_until(next_face) => work.repaint = true,
            () = cut_ended(&mut cut) => work.cut_ended = true,
        }
        while let Ok(command) = commands.try_recv() {
            work.add(command);
        }

        let Ok((done, events)) = spawn_blocking(move || {
            let events = studio.work(work);
            (studio, events)
        })
        .await
        else {
            return;
        };
        studio = done;
        if let Some(helper) = studio.started_cut.take() {
            cut = Some(spawn_blocking(move || depth::wait_for(helper)));
        }
        for event in events {
            if proxy.message(event).is_err() {
                return;
            }
        }
        next_face = studio.next_face();
    }
}

/// Once the depth helper under way exits; never while none is.
async fn cut_ended(cut: &mut Option<JoinHandle<()>>) {
    match cut {
        Some(helper) => {
            let _ = helper.await;
            *cut = None;
        }
        None => pending().await,
    }
}

/// The latest request of each kind.
#[derive(Default)]
struct Work {
    layout: Option<LayoutRequest>,
    frost: Option<ClockFrame>,
    repaint: bool,
    backdrop: Option<BackdropRequest>,
    thumbnails: [Option<ThumbnailRequest>; Shelf::ALL.len()],
    cut_ended: bool,
}

impl Work {
    fn add(&mut self, command: StudioCommand) {
        match command {
            StudioCommand::Layout(request) => self.layout = Some(*request),
            StudioCommand::Frost(frame) => self.frost = Some(frame),
            StudioCommand::Backdrop(request) => self.backdrop = Some(request),
            StudioCommand::Thumbnails(request) => {
                let shelf = request.shelf as usize;
                self.thumbnails[shelf] = Some(*request);
            }
        }
    }
}

#[derive(Default)]
struct Studio {
    painter: Option<Painter>,
    user: Option<User>,
    layout: Option<LayoutRequest>,
    /// What the canvas was last sent of the layout.
    laid: Option<Laid>,
    /// Layouts painted so far.
    layouts: u64,
    /// Where the canvas last said it shows the clock.
    shown: Option<ClockFrame>,
    backdrop_request: Option<BackdropRequest>,
    sources: Vec<Source>,
    /// What each [`Slot`] was last sent, so an unchanged one is not sent again.
    sent: [Option<SlotKey>; Slot::ALL.len()],
    /// The lock's background, blurred for the glass digits to show.
    glass: Option<(GlassKey, Pixmap)>,
    /// The picture the depth helper is cutting.
    cutting: Option<PathBuf>,
    /// The last picture the helper could not cut.
    cut_failed: Option<PathBuf>,
    /// A helper just started, for the loop to wait on.
    started_cut: Option<Child>,
    depth: DepthState,
    /// The background's subject, as the canvas was last sent it.
    subject: Option<Pixmap>,
    /// Where the clock's digits sit, in logical pixels.
    clock_area: Option<Rect>,
}

impl Studio {
    fn work(&mut self, work: Work) -> Vec<StudioEvent> {
        let mut events = Vec::new();
        if work.cut_ended {
            self.cut_ended();
        }
        let backdrop = work.backdrop.or_else(|| {
            work.cut_ended
                .then(|| self.backdrop_request.clone())
                .flatten()
        });
        let relaid = work.layout.is_some();
        if let Some(request) = work.layout {
            self.layout = Some(request);
        }
        let moved = work.frost.is_some_and(|frame| self.shown != Some(frame));
        self.shown = work.frost.or(self.shown);
        let mut refrosted = false;
        let backdrop_asked = backdrop.is_some();
        if let Some(request) = backdrop {
            refrosted = self
                .backdrop_request
                .as_ref()
                .is_none_or(|known| !behind_the_same_glass(known, &request));
            self.backdrop(&request, &mut events);
            self.backdrop_request = Some(request);
        }
        let repainted = relaid || work.repaint || refrosted;
        if repainted {
            events.extend(self.paint());
        } else if moved {
            events.extend(self.refrost());
        }
        if repainted || moved || backdrop_asked {
            events.push(self.told_depth());
        }
        for request in work.thumbnails.into_iter().flatten() {
            events.push(self.thumbnails(&request));
        }
        events
    }

    fn painter(&mut self) -> &mut Painter {
        self.painter.get_or_insert_with(new_painter)
    }

    fn paint(&mut self) -> Option<StudioEvent> {
        let request = self.layout.take()?;
        let painted = self.paint_layout(&request);
        self.layout = Some(request);
        Some(painted)
    }

    fn paint_layout(&mut self, request: &LayoutRequest) -> StudioEvent {
        let face = ClockFace::now(&request.scene.config);
        let sprites = self.lay_out(request, &face);
        let tallest = tallest_glass(&sprites);
        self.ready_glass(
            frost_sigma(tallest * request.scale),
            &request.scene.theme,
            request.draft,
        );
        self.layouts += 1;
        let mut laid = Laid::new(self.layouts, request, sprites);
        let previous = self.laid.take();
        let backdrop = self.glass.as_ref().map(|(_, pixmap)| pixmap);
        let painter = self.painter.get_or_insert_with(new_painter);
        let mut sprites = laid.paint(painter, previous, backdrop.is_some());
        let shown = self.shown.unwrap_or(laid.frame);
        for (id, face) in backdrop
            .map(|backdrop| laid.frosted(backdrop, shown))
            .into_iter()
            .flatten()
        {
            if let Some(sprite) = sprites.iter_mut().find(|sprite| sprite.id == id) {
                sprite.pixmap = Some(face);
            }
        }
        self.clock_area = laid.clock_area(shown);
        let event = StudioEvent::Sprites {
            layout: laid.layout,
            scale: request.scale,
            frame: laid.frame,
            sprites,
        };
        self.laid = Some(laid);
        event
    }

    /// The glass of the layout the canvas has, frosted again for where it
    /// now shows the clock.
    fn refrost(&mut self) -> Option<StudioEvent> {
        let (laid, shown) = (self.laid.as_ref()?, self.shown?);
        self.clock_area = laid.clock_area(shown);
        let (_, backdrop) = self.glass.as_ref()?;
        Some(StudioEvent::Frosted {
            layout: laid.layout,
            faces: laid.frosted(backdrop, shown),
        })
    }

    fn lay_out(&mut self, request: &LayoutRequest, face: &ClockFace) -> Vec<Sprite> {
        let painter = self.painter.get_or_insert_with(new_painter);
        let user = self.user.get_or_insert_with(sample_user);
        let live = Live {
            face,
            user: request.scene.config.show_user.then_some(&*user),
            password_chars: SAMPLE_PASSWORD_CHARS,
            checking: false,
            notice: None,
            caps_lock: false,
        };
        painter.layout(&request.scene, live, &(), request.output)
    }

    fn next_face(&self) -> Option<Instant> {
        let config = &self.layout.as_ref()?.scene.config;
        Some(Instant::now() + until_next_change(config, chrono::Local::now()))
    }
}

/// Whether the glass digits see the same background through both requests.
fn behind_the_same_glass(known: &BackdropRequest, request: &BackdropRequest) -> bool {
    known.background == request.background
        && known.desktop == request.desktop
        && known.pixels == request.pixels
        && frost_darkening(known.dim) == frost_darkening(request.dim)
}

fn new_painter() -> Painter {
    Painter::new(FontSystem::new())
}

fn sample_user() -> User {
    User::current().unwrap_or_else(|_| User {
        login: "you".to_owned(),
        display_name: "You".to_owned(),
    })
}

/// Everything these requests produce, here and now.
#[cfg(test)]
pub fn paint_now(
    layout: LayoutRequest,
    shown: ClockFrame,
    backdrop: BackdropRequest,
    thumbnails: [ThumbnailRequest; Shelf::ALL.len()],
) -> Vec<StudioEvent> {
    Studio::default().work(Work {
        layout: Some(layout),
        frost: Some(shown),
        backdrop: Some(backdrop),
        thumbnails: thumbnails.map(Some),
        ..Work::default()
    })
}

#[cfg(test)]
mod tests {
    use crownos_config::schema::LockBackground;
    use lockscreen::lockscene::LockScene;
    use tokio::sync::mpsc::unbounded_channel;

    use super::*;

    fn layout() -> Work {
        Work {
            layout: Some(LayoutRequest {
                scene: LockScene::default(),
                output: [1920.0, 1080.0],
                scale: 0.3,
                draft: false,
            }),
            ..Work::default()
        }
    }

    fn flat_backdrop() -> BackdropRequest {
        BackdropRequest {
            desktop: String::new(),
            background: LockBackground::Token("accent.end".to_owned()),
            pixels: [576, 324],
            output: [1920, 1080],
            depth: false,
            dim: 0.0,
        }
    }

    /// How many sprites of a layout came with a picture, and how many without.
    fn pictures(events: &[StudioEvent]) -> Option<(usize, usize)> {
        events.iter().find_map(|event| match event {
            StudioEvent::Sprites { sprites, .. } => {
                let fresh = sprites
                    .iter()
                    .filter(|sprite| sprite.pixmap.is_some())
                    .count();
                Some((fresh, sprites.len() - fresh))
            }
            _ => None,
        })
    }

    #[test]
    fn a_layout_paints_only_what_changed_and_frosts_its_glass_again() {
        let mut studio = Studio::default();
        let first = studio.work(Work {
            backdrop: Some(flat_backdrop()),
            ..layout()
        });
        let (fresh, kept) = pictures(&first).expect("sprites");
        assert_eq!(kept, 0, "nothing is on the canvas yet");
        let faces = studio.laid.as_ref().map_or(0, |laid| laid.frosted_count());
        assert!(faces > 0, "the clock is glass");

        let again = studio.work(layout());
        assert_eq!(
            pictures(&again),
            Some((faces, fresh - faces)),
            "only the glass is sent again"
        );
    }

    #[test]
    fn moving_the_clock_frosts_its_glass_for_where_it_stands() {
        let mut studio = Studio::default();
        studio.work(Work {
            backdrop: Some(flat_backdrop()),
            ..layout()
        });
        let moved = ClockFrame {
            offset: [120.0, -40.0],
            ..ClockFrame::default()
        };
        let frost = |frame| Work {
            frost: Some(frame),
            ..Work::default()
        };
        let events = studio.work(frost(moved));
        let Some(StudioEvent::Frosted { layout, faces }) = events.first() else {
            panic!("a move is answered with frosted glass");
        };
        assert_eq!(*layout, 1);
        assert_eq!(
            faces.len(),
            studio.laid.as_ref().map_or(0, |laid| laid.frosted_count())
        );
        assert!(
            !studio
                .work(frost(moved))
                .iter()
                .any(|event| matches!(event, StudioEvent::Frosted { .. })),
            "the same place is not frosted twice"
        );
    }

    #[tokio::test]
    async fn the_loop_paints_the_latest_request_and_ends_with_its_page() {
        let (proxy, mut events) = unbounded_channel();
        let (commands, receiver) = unbounded_channel();
        let studio = tokio::spawn(run(MessageProxy::from(proxy), receiver));
        for scale in [0.1, 0.25] {
            let request = LayoutRequest {
                scene: LockScene::default(),
                output: [1920.0, 1080.0],
                scale,
                draft: false,
            };
            commands
                .send(StudioCommand::Layout(Box::new(request)))
                .expect("the studio is listening");
        }
        let Some(StudioEvent::Sprites { scale, sprites, .. }) = events.recv().await else {
            panic!("a layout request is answered with sprites");
        };
        assert!(!sprites.is_empty());
        assert_eq!(scale, 0.25, "queued requests collapse into the latest");
        assert!(
            sprites.iter().any(|sprite| sprite.date),
            "the date is marked"
        );

        drop(commands);
        studio.await.expect("the loop ends cleanly");
    }
}
