//! A live preview of the lock screen, drawn by the lock screen's own layout,
//! paint and animation — so it shows what the compositor will — and edited
//! in place, as ColorOS's lock screen editor is.
//!
//! [`studio`] paints off the UI thread; what it sends back is uploaded once
//! and composited every frame by [`compose`] on a canvas, the layer styles of
//! a [`LockFlow`](lockscreen::flow::LockFlow) driven by [`playback`] and the
//! clock carried through any drag by [`editor`].

mod backdrop;
mod compose;
mod editor;
mod frames;
mod playback;
#[cfg(test)]
mod profile;
mod requests;
mod shelves;
mod sprites;
mod studio;
#[cfg(test)]
mod tests;
mod toolbar;

use std::time::Instant;

use crownos_config::schema::{Lock, LockBackground, Theme};
use crownui::ext::DrawList;
use crownui::kit::{self, ToggleItem, toggle_group, use_theme};
use crownui::prelude::{
    Color, Cx, Listeners, Role, Runtime, Semantic, Signal, Size, Styled, View, bind, canvas, vstack,
};
use lockscreen::flow::Frame;
use lockscreen::palette::Palette;
use tokio::sync::mpsc::UnboundedSender;

use crate::layout::{setting_row_content, settings_card_titled, settings_divider};
use crate::state::Store;
use crate::util::bridge::bridge;
use crate::util::signal::set_if_changed;
use crate::util::worker::{mailbox, spawn_worker};
use backdrop::Backdrop;
pub use compose::CORNER_RADIUS;
use compose::{Clock, Look, compose};
pub use editor::EditMode;
use editor::Grab;
use playback::{Mode, Playback};
pub use requests::{FONTS, LAYOUTS};
use shelves::Shelves;
use sprites::SpriteSet;
use studio::DepthState;
pub use toolbar::Sheet;

/// What the preview lays out for when no monitor has reported its size.
const FALLBACK_OUTPUT: [f32; 2] = [1920.0, 1080.0];
/// Logical pixels from the preview's corner to the Clock / Password switch.
const SWITCH_INSET: f32 = 10.0;
/// Logical pixels a wheel line scrolls, for resizing the clock with Ctrl.
const WHEEL_LINE: f32 = 40.0;

/// The preview's handle: its state, shared by the card and the sheets, and
/// the way to play it to the clock or the password prompt.
#[derive(Clone)]
pub struct Preview {
    state: State,
    shows: UnboundedSender<Mode>,
}

#[derive(Clone, Copy)]
struct State {
    store: Store,
    /// The settings app's own theme, which the sheet's pictures are inked in.
    theme: Signal<kit::Theme>,
    /// The canvas, in logical window pixels.
    size: Signal<Size>,
    device_scale: Signal<f32>,
    sprites: Signal<SpriteSet>,
    backdrop: Signal<Backdrop>,
    frame: Signal<Frame>,
    playback: Signal<Playback>,
    mode: Signal<Mode>,
    animating: Signal<bool>,
    unreadable: Signal<bool>,
    depth: Signal<DepthState>,
    sheet: Signal<Sheet>,
    edit_mode: Signal<EditMode>,
    grab: Signal<Option<Grab>>,
    hovering: Signal<bool>,
    /// The instant changing digits are drawn at.
    morph_clock: Signal<Instant>,
    shelves: Signal<Shelves>,
}

impl Preview {
    /// Starts painting for the calling component, which owns everything the
    /// preview puts on the GPU.
    pub fn start(cx: &mut Cx, store: Store) -> Self {
        let now = Instant::now();
        let playback =
            cx.untrack(|runtime| store.section(runtime, |lock: &Lock| Playback::new(lock, now)));
        let state = State {
            store,
            theme: use_theme(cx),
            size: cx.signal(Size::ZERO),
            device_scale: cx.scale_factor(),
            sprites: cx.signal(SpriteSet::default()),
            backdrop: cx.signal(Backdrop::default()),
            frame: cx.signal(playback.frame(now)),
            playback: cx.signal(playback),
            mode: cx.signal(Mode::Clock),
            animating: cx.signal(false),
            unreadable: cx.signal(false),
            depth: cx.signal(DepthState::Off),
            sheet: cx.signal(Sheet::default()),
            edit_mode: cx.signal(EditMode::default()),
            grab: cx.signal(None),
            hovering: cx.signal(false),
            morph_clock: cx.signal(now),
            shelves: cx.signal(Shelves::default()),
        };
        let commands = spawn_worker(cx, studio::run, move |cx, event| state.accept(cx, event));
        state.follow(cx, commands);
        let shows = mailbox(cx, move |cx, mode| state.show_mode(cx, mode));
        Self { state, shows }
    }

    /// Whether the lock's own picture failed to load.
    pub fn picture_unreadable(&self) -> Signal<bool> {
        self.state.unreadable
    }

    /// What dragging the clock does while the Effects sheet is open.
    pub fn edit_mode(&self) -> Signal<EditMode> {
        self.state.edit_mode
    }

    /// The preview, its editing bar, and `sheet` under them.
    pub fn card<S: View>(&self, cx: &mut Cx, sheet: S) -> impl View + use<S> {
        let state = self.state;
        let shows = self.shows.clone();
        let mode = bridge(
            cx,
            move |runtime| {
                let mode = state.mode.get(runtime);
                Mode::ALL
                    .iter()
                    .position(|candidate| *candidate == mode)
                    .unwrap_or_default()
            },
            move |_, index: usize| {
                if let Some(mode) = Mode::ALL.get(index) {
                    let _ = shows.send(*mode);
                }
            },
        );
        let picture = canvas(move |runtime, size, list| state.paint(runtime, size, list))
            .w_full()
            .aspect(bind(move |runtime| {
                let [width, height] = state.output(runtime);
                width / height
            }))
            .rounded(CORNER_RADIUS)
            .role(Role::Image)
            .label("Lock screen preview: drag the clock to move it, its corner to stretch it")
            .on_click(move |cx, event| state.click(cx, event.position))
            .on_drag_start(move |cx, event| state.grab(cx, event.position))
            .on_drag(move |cx, event| state.drag_by(cx, event.total))
            .on_drag_end(move |cx, _| state.release(cx))
            .on_pointer_enter(move |cx, _| set_if_changed(cx, state.hovering, true))
            .on_pointer_leave(move |cx, _| set_if_changed(cx, state.hovering, false))
            .on_wheel(move |cx, event| {
                if event.modifiers.ctrl_key {
                    state.zoom(cx, event.delta.to_pixels(WHEEL_LINE).y);
                    event.prevent_default();
                    event.stop_propagation();
                }
            });
        let switch = toggle_group(Mode::ALL.map(|mode| ToggleItem::text(mode.title())), mode)
            .label("Show")
            .absolute()
            .top(SWITCH_INSET)
            .right(SWITCH_INSET);
        settings_card_titled(
            "Preview",
            (
                setting_row_content(vstack((picture, switch)).relative().w_full()),
                settings_divider(),
                setting_row_content(self.toolbar(cx)),
                settings_divider(),
                sheet,
            ),
        )
    }
}

impl State {
    /// What the lock's settings say about the background, apart from pictures.
    fn look(self, runtime: &mut Runtime) -> Look {
        let (dim, background) = self
            .store
            .section(runtime, |lock: &Lock| (lock.dim, lock.background.clone()));
        let fill = match &background {
            LockBackground::Token(token) => Some(self.store.section(runtime, |theme: &Theme| {
                let [red, green, blue, alpha] = Palette::new(theme).color(token);
                Color::new(red, green, blue, alpha)
            })),
            LockBackground::Desktop | LockBackground::Image(_) => None,
        };
        Look {
            own_background: background != LockBackground::Desktop,
            fill,
            dim,
            depth: self.depth.get(runtime) == DepthState::Ready,
        }
    }

    fn paint(self, runtime: &mut Runtime, size: Size, list: &mut DrawList) {
        set_if_changed(runtime, self.size, size);
        if size.width <= 0.0 || size.height <= 0.0 {
            return;
        }
        let frame = self.frame.get(runtime);
        let output = self.output(runtime);
        let look = self.look(runtime);
        let shown = self.shown_look(runtime).frame;
        let edit = self.clock_edit(runtime, shown);
        let grabbed = self.grab.with(runtime, Option::is_some);
        let mode = self.handle_mode(runtime);
        let wanted = grabbed || self.hovering.get(runtime) || mode != EditMode::Move;
        let handles =
            (wanted && self.mode.get(runtime) == Mode::Clock && !self.animating.get(runtime))
                .then_some(mode);
        let clock = Clock {
            edit: edit.as_ref(),
            handles,
            now: self.morph_clock.get(runtime),
        };
        runtime.with2(self.backdrop, self.sprites, |backdrop, sprites| {
            compose(list, size, output, &look, backdrop, sprites, &frame, &clock);
        });
    }
}
