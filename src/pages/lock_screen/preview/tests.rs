//! Offscreen renders of the preview card, fed by the studio in-line since
//! tests run no task executor.

use std::time::Instant;

use crownos_config::schema::lock::ClockKey;
use crownos_config::schema::{ClockFont, ClockFrame, ClockLayout, ClockLook};
use crownui::ext::{InlineApp, TextSystem};
use crownui::kit::{INTER_FAMILY, inter_font};
use crownui::prelude::{Cx, Size, Styled, View, canvas, component, vstack};
use impeller::{ImpellerBackend, Offscreen};
use lockscreen::transition::{Moments, Spring};
use taffy_layout::TaffyEngine;

use super::editor::EditMode;
use super::playback::{Mode, Playback};
use super::studio::{Shelf, paint_now};
use super::{FALLBACK_OUTPUT, Preview, Sheet, State};
use crate::net::outputs::{Mode as OutputMode, Monitor, Transform as OutputTransform};
use crate::pages::lock_screen::editor;
use crate::state::Store;

const WIDTH: f32 = 760.0;
const HEIGHT: f32 = 1180.0;
/// The canvas inside the card inside the padding.
const CANVAS_WIDTH: f32 = WIDTH - 2.0 * 24.0 - 2.0 * 18.0 - 2.0;

#[derive(Debug, Clone, Copy)]
enum Shot {
    /// The default clock, at rest.
    Clock,
    /// The pointer over the preview: the outline and its stretch handle.
    Editor,
    Password,
    RevealHalfway,
    /// Stacked serif digits, warped, with the Effects sheet's warp corners.
    Warped,
    /// The Wallpaper sheet, the clock at the left, clear of the subject.
    Wallpaper,
    /// The clock moved into the subject, which gives way so it stays legible.
    HeldBack,
}

impl Shot {
    const ALL: [Self; 7] = [
        Self::Clock,
        Self::Editor,
        Self::Password,
        Self::RevealHalfway,
        Self::Warped,
        Self::Wallpaper,
        Self::HeldBack,
    ];

    fn name(self) -> String {
        format!("{self:?}").to_lowercase()
    }

    fn look(self) -> ClockLook {
        match self {
            Self::Warped => ClockLook {
                layout: ClockLayout::Stacked,
                font: ClockFont::Serif,
                weight: 200.0,
                shadow: 0.7,
                frame: ClockFrame {
                    warp: [[0.0, 0.0], [0.15, -0.1], [0.1, 0.05], [0.0, 0.0]],
                    minutes: [40.0, 0.0],
                    ..ClockFrame::default()
                },
                ..ClockLook::default()
            },
            Self::Wallpaper => ClockLook {
                layout: ClockLayout::Left,
                ..ClockLook::default()
            },
            Self::HeldBack => ClockLook {
                frame: ClockFrame {
                    offset: [420.0, 260.0],
                    ..ClockFrame::default()
                },
                ..ClockLook::default()
            },
            _ => ClockLook::default(),
        }
    }

    fn sheet(self) -> Sheet {
        match self {
            Self::Warped => Sheet::Effects,
            Self::Wallpaper => Sheet::Wallpaper,
            _ => Sheet::Clock,
        }
    }
}

/// Writes `$CROWNSETTINGS_RENDER_DIR/lock-preview-<shot>.png` over the
/// picture `$CROWNSETTINGS_RENDER_WALLPAPER` names, with the depth cut cached
/// for it if there is one; `$CROWNSETTINGS_RENDER_SHOTS` names the shots to
/// take, comma-separated, all of them by default. Each shot edits the
/// settings, so it runs only with a scratch `CROWN_CONFIG_DIR`. One shot per
/// process: the offscreen renderer crashes as the process ends, after the
/// picture is written.
#[test]
#[ignore = "needs a GPU and a scratch CROWN_CONFIG_DIR; writes PNGs for manual inspection"]
fn render_the_preview() {
    let (Some(dir), Some(desktop), Some(_)) = (
        std::env::var_os("CROWNSETTINGS_RENDER_DIR"),
        std::env::var("CROWNSETTINGS_RENDER_WALLPAPER").ok(),
        std::env::var_os(crownos_config::CONFIG_DIR_ENV),
    ) else {
        return;
    };
    let wanted = std::env::var("CROWNSETTINGS_RENDER_SHOTS").unwrap_or_default();
    for shot in Shot::ALL {
        if !wanted.is_empty() && !wanted.split(',').any(|name| name == shot.name()) {
            continue;
        }
        let desktop = desktop.clone();
        let root = component(move |cx| {
            let store = Store::defaults(cx);
            store.set(
                cx,
                crownos_config::schema::appearance::Wallpaper,
                desktop.clone(),
            );
            store.set(cx, ClockKey, shot.look());
            let preview = Preview::start(cx, store);
            let state = preview.state;
            state.sheet.set(cx, shot.sheet());
            if let Shot::Warped = shot {
                state.edit_mode.set(cx, EditMode::Warp);
            }
            if let Shot::Editor = shot {
                state.hovering.set(cx, true);
            }
            let height = CANVAS_WIDTH * FALLBACK_OUTPUT[1] / FALLBACK_OUTPUT[0];
            feed_studio(cx, state, Size::new(CANVAS_WIDTH, height));
            match shot {
                Shot::Password => state.show_mode(cx, Mode::Password),
                Shot::RevealHalfway => {
                    let (lock, now) = (Default::default(), Instant::now());
                    let mut playback = Playback::new(&lock, now);
                    playback.show(&lock, Mode::Password, now);
                    state.show(cx, playback.frame(now + Spring::LOCK.settle_time() / 4));
                }
                _ => {}
            }
            vstack(editor(cx, store, &preview)).w(WIDTH).p(24.0)
        });
        let name = format!("lock-preview-{}.png", shot.name());
        render(
            root,
            Size::new(WIDTH, HEIGHT),
            &std::path::Path::new(&dir).join(name),
        );
    }
}

/// Writes `$CROWNSETTINGS_RENDER_DIR/lock-moment-<moment>.png`: the canvas
/// alone, pixel for pixel as big as the `<width>x<height>` output
/// `$CROWNSETTINGS_RENDER_OUTPUT` names, showing the settings in the scratch
/// `CROWN_CONFIG_DIR` at rest on the clock — or on the prompt, with
/// `$CROWNSETTINGS_RENDER_MOMENT=prompt` — to hold against what the lock
/// screen draws on such an output. One moment per process.
#[test]
#[ignore = "needs a GPU and a scratch CROWN_CONFIG_DIR; writes a PNG to compare with the lock"]
fn render_a_lock_moment() {
    let (Some(dir), Some(output), Some(_)) = (
        std::env::var_os("CROWNSETTINGS_RENDER_DIR"),
        std::env::var("CROWNSETTINGS_RENDER_OUTPUT")
            .ok()
            .and_then(|size| parse_output(&size)),
        std::env::var_os(crownos_config::CONFIG_DIR_ENV),
    ) else {
        return;
    };
    let prompt =
        std::env::var("CROWNSETTINGS_RENDER_MOMENT").is_ok_and(|moment| moment == "prompt");
    let size = Size::new(output[0] as f32, output[1] as f32);
    let root = component(move |cx| {
        let store = Store::load(cx);
        store
            .outputs
            .update(cx, |outputs| outputs.monitors = vec![monitor(output)]);
        let state = Preview::start(cx, store).state;
        feed_studio(cx, state, size);
        let moment = if prompt {
            Moments::PROMPT
        } else {
            Moments::CLOCK
        };
        state.show(cx, moment.frame(0.0));
        canvas(move |runtime, size, list| state.paint(runtime, size, list))
            .w(size.width)
            .h(size.height)
    });
    let name = format!(
        "lock-moment-{}.png",
        if prompt { "prompt" } else { "clock" }
    );
    render(root, size, &std::path::Path::new(&dir).join(name));
}

/// `<width>x<height>`, in logical pixels.
fn parse_output(size: &str) -> Option<[i32; 2]> {
    let (width, height) = size.split_once('x')?;
    Some([width.parse().ok()?, height.parse().ok()?])
}

/// One monitor showing `size` logical pixels at scale 1.
fn monitor([width, height]: [i32; 2]) -> Monitor {
    Monitor {
        name: "render".to_owned(),
        description: String::new(),
        make: None,
        model: None,
        serial: None,
        enabled: true,
        modes: vec![OutputMode {
            width,
            height,
            refresh: 60_000,
            preferred: true,
        }],
        current_mode: Some(0),
        position: (0, 0),
        scale: 1.0,
        transform: OutputTransform::Normal,
        adaptive_sync: false,
        vrr_capable: false,
    }
}

/// Renders `root` offscreen at `size` and writes it to `path` as a PNG.
fn render(root: impl View, size: Size, path: &std::path::Path) {
    let mut app = offscreen(root, size);
    app.settle().expect("a frame renders");
    let pixels = app.renderer_mut().read_pixels().expect("pixels read back");
    let png = impeller::encode_png(&pixels).expect("PNG encodes");
    std::fs::write(path, png).expect("PNG written");
}

/// Paints what the preview would ask the studio for with a `size` canvas,
/// in-line, and hands the result over.
fn feed_studio(cx: &mut Cx, state: State, size: Size) {
    state.size.set(cx, size);
    let (Some(layout), Some(backdrop)) =
        (state.layout_request(cx, None), state.backdrop_request(cx))
    else {
        return;
    };
    let thumbnails = Shelf::ALL.map(|shelf| state.thumbnail_request(cx, shelf));
    let shown = cx.untrack(|runtime| state.shown_look(runtime).frame);
    for event in paint_now(layout, shown, backdrop, thumbnails) {
        state.accept(cx, event);
    }
}

fn offscreen(root: impl View, size: Size) -> InlineApp<ImpellerBackend, TaffyEngine> {
    let surface = Offscreen {
        width: size.width as u32,
        height: size.height as u32,
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
    app
}
