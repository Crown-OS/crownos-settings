//! What the studio is asked to paint, kept in step with the settings, the
//! drag under way, the monitors and the canvas.
//!
//! A drag that moves, resizes, splits or warps the clock is the canvas's to
//! draw: the studio is only told where the clock is shown, to frost its glass
//! there. Only a drag that changes the glyphs themselves, a stretch or the
//! colon, lays the clock out again while it lasts.

use crownos_config::schema::lock::{Background, Dim};
use crownos_config::schema::{
    ClockFont, ClockFrame, ClockLayout, ClockLook, Lock, Theme, appearance,
};
use crownui::prelude::{Cx, Runtime};
use lockscreen::lockscene::LockScene;
use tokio::sync::mpsc::UnboundedSender;

use super::studio::{BackdropRequest, LayoutRequest, Shelf, StudioCommand, ThumbnailRequest};
use super::{FALLBACK_OUTPUT, State};

/// The layouts the Clock & colour sheet offers, in the order it shows them.
pub const LAYOUTS: [(ClockLayout, &str); 3] = [
    (ClockLayout::Centered, "Centered"),
    (ClockLayout::Left, "Left"),
    (ClockLayout::Stacked, "Stacked"),
];
/// The fonts the Clock & colour sheet offers, in the order it shows them.
pub const FONTS: [(ClockFont, &str); 3] = [
    (ClockFont::Sans, "Sans"),
    (ClockFont::Grotesk, "Grotesk"),
    (ClockFont::Serif, "Serif"),
];
/// Logical pixels across a layout card's picture.
const LAYOUT_PIXELS: f32 = 220.0;
/// Logical pixels across a font tile's picture.
const FONT_PIXELS: f32 = 90.0;

impl State {
    /// Keeps the studio's requests in step with the settings and the canvas.
    pub(super) fn follow(self, cx: &mut Cx, commands: UnboundedSender<StudioCommand>) {
        let drafted = cx.memo(move |runtime| {
            self.grab.with(runtime, |grab| {
                grab.as_ref().map(|grab| shape_of(&grab.look))
            })
        });
        let layout = cx.memo(move |runtime| {
            let draft = drafted.get(runtime);
            self.layout_request(runtime, draft)
        });
        let shown = cx.memo(move |runtime| self.shown_look(runtime).frame);
        let backdrop = cx.memo(move |runtime| self.backdrop_request(runtime));
        for shelf in Shelf::ALL {
            let request = cx.memo(move |runtime| self.thumbnail_request(runtime, shelf));
            let sender = commands.clone();
            cx.effect(move |runtime| {
                let request = request.get(runtime);
                let _ = sender.send(StudioCommand::Thumbnails(Box::new(request)));
            });
        }
        let sender = commands.clone();
        cx.effect(move |runtime| {
            if let Some(request) = layout.get(runtime) {
                let _ = sender.send(StudioCommand::Layout(Box::new(request)));
            }
        });
        let sender = commands.clone();
        cx.effect(move |runtime| {
            let _ = sender.send(StudioCommand::Frost(shown.get(runtime)));
        });
        cx.effect(move |runtime| {
            if let Some(request) = backdrop.get(runtime) {
                let _ = commands.send(StudioCommand::Backdrop(request));
            }
        });
    }

    /// The scene as the settings have it, with the glyphs of `draft`, the
    /// shape of a drag's look, while it differs.
    pub(super) fn layout_request(
        self,
        runtime: &mut Runtime,
        draft: Option<ClockLook>,
    ) -> Option<LayoutRequest> {
        let size = self.size.get(runtime);
        if size.width < 1.0 {
            return None;
        }
        let output = self.output(runtime);
        let scale = size.width / output[0] * self.device_scale.get(runtime);
        let mut config = self.store.section(runtime, laid_out);
        let draft = draft.filter(|shape| *shape != shape_of(&config.clock));
        if let Some(shape) = &draft {
            config.clock = ClockLook {
                frame: config.clock.frame,
                ..shape.clone()
            };
        }
        let theme = self.store.section(runtime, Theme::clone);
        Some(LayoutRequest {
            scene: LockScene {
                config,
                theme,
                ..LockScene::default()
            },
            output,
            scale,
            draft: draft.is_some(),
        })
    }

    pub(super) fn thumbnail_request(self, runtime: &mut Runtime, shelf: Shelf) -> ThumbnailRequest {
        let output = self.output(runtime);
        let mut base = self.store.section(runtime, laid_out);
        base.clock.frame = ClockFrame::default();
        let theme = self.store.section(runtime, Theme::clone);
        let ink = self
            .theme
            .with(runtime, |theme| theme.text.primary.to_rgba8());
        let scene = |change: &dyn Fn(&mut ClockLook)| {
            let mut config = base.clone();
            change(&mut config.clock);
            LockScene {
                config,
                theme: theme.clone(),
                ..LockScene::default()
            }
        };
        let (scenes, pixels) = match shelf {
            Shelf::Layouts => (
                LAYOUTS
                    .iter()
                    .map(|(layout, _)| scene(&|clock| clock.layout = *layout))
                    .collect(),
                LAYOUT_PIXELS,
            ),
            Shelf::Fonts => (
                FONTS
                    .iter()
                    .map(|(font, _)| {
                        scene(&|clock| {
                            clock.font = *font;
                            clock.stretch = ClockLook::MIN_STRETCH;
                        })
                    })
                    .collect(),
                FONT_PIXELS,
            ),
        };
        ThumbnailRequest {
            shelf,
            scenes,
            output,
            pixels: pixels * self.device_scale.get(runtime),
            ink,
        }
    }

    pub(super) fn backdrop_request(self, runtime: &mut Runtime) -> Option<BackdropRequest> {
        let size = self.size.get(runtime);
        let device_scale = self.device_scale.get(runtime);
        let pixels = [size.width, size.height].map(|side| (side * device_scale).round() as u32);
        if pixels.contains(&0) {
            return None;
        }
        Some(BackdropRequest {
            desktop: self.store.get(runtime, appearance::Wallpaper),
            background: self.store.get(runtime, Background),
            pixels,
            output: self.output(runtime).map(|side| side as u32),
            depth: self.store.section(runtime, |lock: &Lock| lock.clock.depth),
            dim: self.store.get(runtime, Dim),
        })
    }

    /// The logical size of the first monitor in use.
    pub(super) fn output(self, runtime: &mut Runtime) -> [f32; 2] {
        self.store
            .outputs
            .with(runtime, |outputs| {
                outputs
                    .monitors
                    .iter()
                    .filter(|monitor| monitor.enabled)
                    .map(|monitor| monitor.logical_size())
                    .find(|(width, height)| *width > 0 && *height > 0)
            })
            .map_or(FALLBACK_OUTPUT, |(width, height)| {
                [width as f32, height as f32]
            })
    }
}

/// Only what the layout reads, so editing anything else — the background,
/// the clock's warp, its depth — does not lay the screen out again.
fn laid_out(lock: &Lock) -> Lock {
    Lock {
        clock: laid_clock(&lock.clock),
        time_format: lock.time_format.clone(),
        date_format: lock.date_format.clone(),
        auth_field: lock.auth_field,
        slots: lock.slots.clone(),
        show_user: lock.show_user,
        ..Lock::default()
    }
}

/// The clock as the layout reads it: the warp and the depth are the canvas's.
fn laid_clock(look: &ClockLook) -> ClockLook {
    let mut clock = look.clone();
    clock.frame.warp = ClockFrame::default().warp;
    clock.depth = ClockLook::default().depth;
    clock
}

/// What the clock's glyphs are drawn as, wherever the canvas shows them.
fn shape_of(look: &ClockLook) -> ClockLook {
    ClockLook {
        frame: ClockFrame::default(),
        depth: ClockLook::default().depth,
        ..look.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use crownos_config::schema::lock::ClockKey;
    use crowntest::TestApp;
    use crownui::prelude::{Size, component, vstack};

    use super::*;
    use crate::pages::lock_screen::preview::Preview;
    use crate::state::Store;

    fn mounted() -> (TestApp, State) {
        let state = Rc::new(Cell::new(None));
        let mount = Rc::clone(&state);
        let mut app = TestApp::new(component(move |cx| {
            let store = Store::defaults(cx);
            mount.set(Some(Preview::start(cx, store).state));
            vstack(())
        }));
        let state = state.get().expect("the preview mounts");
        state.size.set(app.ui().cx(), Size::new(600.0, 338.0));
        (app, state)
    }

    #[test]
    fn only_a_drag_that_changes_the_glyphs_lays_the_clock_out_again() {
        let (mut app, state) = mounted();
        let cx = app.ui().cx();
        let at_rest = state.layout_request(cx, None);
        let look = cx.untrack(|runtime| state.store.get(runtime, ClockKey));
        let placed = ClockLook {
            frame: ClockFrame {
                offset: [80.0, -30.0],
                scale: 1.4,
                warp: [[0.1, 0.0], [0.0; 2], [0.0; 2], [0.0; 2]],
                hours: [0.0, 12.0],
                minutes: [6.0, 0.0],
            },
            ..look.clone()
        };
        assert_eq!(
            state.layout_request(cx, Some(shape_of(&placed))),
            at_rest,
            "moving, resizing, warping and splitting are the canvas's"
        );
        let stretched = ClockLook {
            stretch: look.clamped_stretch() + 1.0,
            ..look
        };
        let draft = state
            .layout_request(cx, Some(shape_of(&stretched)))
            .expect("a canvas to lay out for");
        assert!(draft.draft);
        assert_eq!(draft.scene.config.clock.stretch, stretched.stretch);
    }

    #[test]
    fn the_wheel_resizes_the_clock_as_a_draft() {
        let (mut app, state) = mounted();
        let cx = app.ui().cx();
        let stored = cx.untrack(|runtime| state.store.get(runtime, ClockKey));
        state.zoom(cx, -50.0);
        state.zoom(cx, -50.0);
        let shown = cx.untrack(|runtime| state.shown_look(runtime));
        assert!(shown.frame.scale > stored.frame.scale);
        assert_eq!(
            cx.untrack(|runtime| state.store.get(runtime, ClockKey)),
            stored,
            "the settings wait for the wheel to rest"
        );
        state.release(cx);
        assert_eq!(
            cx.untrack(|runtime| state.store.get(runtime, ClockKey)),
            shown
        );
    }
}
