//! Offscreen renders of the preview, fed by the studio in-line since tests
//! run no task executor.

use std::time::{Duration, Instant};

use crownos_config::schema::lock::ClockKey;
use crownos_config::schema::{
    Appearance, ClockArrangement, ClockFrame, ClockLook, Lock, LockBackground, Theme,
};
use crownui::ext::{InlineApp, TextSystem};
use crownui::kit::{INTER_FAMILY, inter_font};
use crownui::prelude::{Size, Styled, component, vstack};
use impeller::{ImpellerBackend, Offscreen};
use lockscreen::lockscene::LockScene;
use taffy_layout::TaffyEngine;

use super::editor::EditMode;
use super::playback::{Mode, Playback};
use super::studio::{BackdropRequest, LayoutRequest, paint_now};
use super::{FALLBACK_OUTPUT, Preview, Request};
use crate::pages::lock_screen::moment::Moment;
use crate::state::Store;

const WIDTH: f32 = 760.0;
const HEIGHT: f32 = 560.0;
/// The canvas inside the card inside the padding.
const CANVAS: [f32; 2] = [WIDTH - 2.0 * 24.0 - 2.0 * 18.0 - 2.0, 0.0];

#[derive(Clone, Copy)]
enum Shot {
    Clock,
    Password,
    RevealHalfway,
    /// Tall glass digits, stacked, warped, with the warp handles out.
    Warped,
}

fn warped_look() -> ClockLook {
    ClockLook {
        arrangement: ClockArrangement::Stacked,
        weight: Some(200),
        stretch: 2.2,
        glass: true,
        shadow: 0.7,
        frame: ClockFrame {
            warp: [[0.0, 0.0], [0.15, -0.1], [0.1, 0.05], [0.0, 0.0]],
            minutes: [40.0, 0.0],
            ..ClockFrame::default()
        },
        ..ClockLook::default()
    }
}

/// Writes `$CROWNSETTINGS_RENDER_DIR/lock-preview-<shot>.png` over the
/// desktop wallpaper in the real `appearance.ron`.
#[test]
#[ignore = "needs a GPU; writes PNGs for manual inspection"]
fn render_the_preview() {
    let Some(dir) = std::env::var_os("CROWNSETTINGS_RENDER_DIR") else {
        return;
    };
    let wallpaper = crownos_config::load::<Appearance>(Appearance::SECTION).wallpaper;
    for (shot, name) in [
        (Shot::Warped, "warped"),
        (Shot::Clock, "clock"),
        (Shot::Password, "password"),
        (Shot::RevealHalfway, "reveal-halfway"),
    ] {
        let wallpaper = wallpaper.clone();
        let root = component(move |cx| {
            let store = Store::defaults(cx);
            let preview = Preview::start(cx, store);
            let state = preview.state;
            let mut laid = Lock::default();
            if let Shot::Warped = shot {
                store.set(cx, ClockKey, warped_look());
                state.edit_mode.set(cx, EditMode::Warp);
                laid.clock = warped_look();
                laid.clock.frame.warp = ClockFrame::default().warp;
            }
            let width = CANVAS[0];
            let height = width * FALLBACK_OUTPUT[1] / FALLBACK_OUTPUT[0];
            let events = paint_now(
                LayoutRequest {
                    scene: LockScene {
                        config: laid,
                        theme: Theme::default(),
                        ..LockScene::default()
                    },
                    output: FALLBACK_OUTPUT,
                    scale: width / FALLBACK_OUTPUT[0],
                },
                BackdropRequest {
                    desktop: wallpaper.clone(),
                    background: LockBackground::Desktop,
                    pixels: [width as u32, height as u32],
                    output: FALLBACK_OUTPUT.map(|side| side as u32),
                    blur: 1.0,
                    depth: None,
                    dim: 0.25,
                },
            );
            for event in events {
                state.accept(cx, event);
            }
            let lock = Lock::default();
            match shot {
                Shot::Clock | Shot::Warped => {}
                Shot::Password => state.handle(cx, Request::Show(Mode::Password)),
                Shot::RevealHalfway => {
                    let now = Instant::now();
                    let mut playback = Playback::new(&lock, now);
                    playback.play(&lock, Moment::Reveal, now);
                    let halfway = now
                        + Duration::from_millis(u64::from(lock.transitions.reveal.duration_ms / 2));
                    state.show(cx, playback.frame(halfway));
                }
            }
            vstack(preview.card(cx)).w(WIDTH).p(24.0)
        });
        let size = Size::new(WIDTH, HEIGHT);
        let surface = Offscreen {
            width: WIDTH as u32,
            height: HEIGHT as u32,
            scale: 1.0,
        };
        let mut app = InlineApp::<ImpellerBackend, TaffyEngine>::with_surface(
            root,
            TaffyEngine::new(),
            size,
            1.0,
            &surface,
        )
        .expect("an offscreen renderer");
        app.text_cache()
            .system_mut()
            .register_font(inter_font().to_vec(), Some(INTER_FAMILY))
            .expect("Inter registers");
        app.settle().expect("a frame renders");
        let pixels = app.renderer_mut().read_pixels().expect("pixels read back");
        let png = impeller::encode_png(&pixels).expect("PNG encodes");
        std::fs::write(
            std::path::Path::new(&dir).join(format!("lock-preview-{name}.png")),
            png,
        )
        .expect("PNG written");
    }
}
