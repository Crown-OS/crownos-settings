//! The preview's painter: the lock screen's own layout and paint, and the
//! pictures behind it, made off the UI thread — loading the system's fonts
//! alone takes a noticeable while.
//!
//! Requests that pile up while it works collapse into the latest of each
//! kind, and the clock is repainted by itself whenever its face would change.

mod messages;
mod pictures;

use std::time::Instant;

use lockscreen::clock::{ClockFace, until_next_change};
use lockscreen::cosmic_text::FontSystem;
use lockscreen::layout::{Content, Live};
use lockscreen::paint::Painter;
use lockscreen::user::User;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::util::deadline::sleep_until;
use crate::util::worker::MessageProxy;
pub use messages::{
    BackdropRequest, LayoutRequest, Painted, Picture, Slot, StudioCommand, StudioEvent,
};
use pictures::{SlotKey, Source};

/// Dots in the sample password, enough to read as one.
const SAMPLE_PASSWORD_CHARS: usize = 6;

pub async fn run(proxy: MessageProxy<StudioEvent>, mut commands: UnboundedReceiver<StudioCommand>) {
    let mut studio = Studio::default();
    let mut next_face = None;
    loop {
        let mut work = Work::default();
        tokio::select! {
            command = commands.recv() => match command {
                Some(command) => work.add(command),
                None => return,
            },
            () = sleep_until(next_face) => work.repaint = true,
        }
        while let Ok(command) = commands.try_recv() {
            work.add(command);
        }
        let Ok((done, events)) = tokio::task::spawn_blocking(move || {
            let events = studio.work(work);
            (studio, events)
        })
        .await
        else {
            return;
        };
        studio = done;
        for event in events {
            if proxy.message(event).is_err() {
                return;
            }
        }
        next_face = studio.next_face();
    }
}

/// The latest request of each kind.
#[derive(Default)]
struct Work {
    layout: Option<LayoutRequest>,
    repaint: bool,
    backdrop: Option<BackdropRequest>,
}

impl Work {
    fn add(&mut self, command: StudioCommand) {
        match command {
            StudioCommand::Layout(request) => self.layout = Some(*request),
            StudioCommand::Backdrop(request) => self.backdrop = Some(request),
        }
    }
}

#[derive(Default)]
struct Studio {
    painter: Option<Painter>,
    user: Option<User>,
    layout: Option<LayoutRequest>,
    sources: Vec<Source>,
    /// What each [`Slot`] was last sent, so an unchanged one is not sent again.
    sent: [Option<SlotKey>; Slot::ALL.len()],
}

impl Studio {
    fn work(&mut self, work: Work) -> Vec<StudioEvent> {
        let relaid = work.layout.is_some();
        if let Some(request) = work.layout {
            self.layout = Some(request);
        }
        let mut events = Vec::new();
        if relaid || work.repaint {
            events.extend(self.paint());
        }
        if let Some(request) = work.backdrop {
            self.backdrop(&request, &mut events);
        }
        events
    }

    fn paint(&mut self) -> Option<StudioEvent> {
        let request = self.layout.as_ref()?;
        let painter = self
            .painter
            .get_or_insert_with(|| Painter::new(FontSystem::new()));
        let user = self.user.get_or_insert_with(sample_user);
        let face = ClockFace::now(&request.scene.config);
        let live = Live {
            face: &face,
            user: request.scene.config.show_user.then_some(&*user),
            password_chars: SAMPLE_PASSWORD_CHARS,
            checking: false,
            notice: None,
            caps_lock: false,
        };
        let sprites = painter
            .layout(&request.scene, live, &(), request.output)
            .into_iter()
            .filter_map(|sprite| {
                let pad = (sprite.reach() * request.scale).ceil();
                let pixmap = painter.paint(&sprite, request.scale, pad as u32, &())?;
                Some(Painted {
                    id: sprite.id,
                    layer: sprite.layer,
                    part: sprite.part,
                    rect: sprite.rect,
                    pad: pad / request.scale,
                    glyph: match &sprite.content {
                        Content::Glyph(run) => Some(run.ch),
                        _ => None,
                    },
                    pixmap,
                })
            })
            .collect();
        Some(StudioEvent::Sprites {
            scale: request.scale,
            frame: request.scene.config.clock.frame,
            sprites,
        })
    }

    fn next_face(&self) -> Option<Instant> {
        let config = &self.layout.as_ref()?.scene.config;
        Some(Instant::now() + until_next_change(config, chrono::Local::now()))
    }
}

fn sample_user() -> User {
    User::current().unwrap_or_else(|_| User {
        login: "you".to_owned(),
        display_name: "You".to_owned(),
    })
}

/// Everything a layout and a backdrop request produce, here and now.
#[cfg(test)]
pub fn paint_now(layout: LayoutRequest, backdrop: BackdropRequest) -> Vec<StudioEvent> {
    Studio::default().work(Work {
        layout: Some(layout),
        repaint: false,
        backdrop: Some(backdrop),
    })
}

#[cfg(test)]
mod tests {
    use lockscreen::lockscene::LockScene;
    use tokio::sync::mpsc::unbounded_channel;

    use super::*;

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

        drop(commands);
        studio.await.expect("the loop ends cleanly");
    }
}
