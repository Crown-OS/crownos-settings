//! A live preview of the lock screen, drawn by the lock screen's own layout,
//! paint and transitions — so it shows what the compositor will.
//!
//! [`studio`] paints off the UI thread; what it sends back is uploaded once
//! and composited every frame by [`compose`] on a canvas, the layer styles of
//! a [`LockFlow`](lockscreen::flow::LockFlow) driven by [`playback`].

mod backdrop;
mod compose;
mod editor;
mod frames;
mod playback;
mod requests;
mod sprites;
mod studio;
#[cfg(test)]
mod tests;

use std::time::{Duration, Instant};

use crownos_config::schema::{Lock, LockBackground, Theme};
use crownui::ext::DrawList;
use crownui::kit::{ButtonOptions, ButtonSize, ToggleItem, button, icons, toggle_group};
use crownui::prelude::{
    Color, Cx, Listeners, Role, Runtime, Semantic, Signal, Size, Styled, View, bind, canvas, spacer,
};
use lockscreen::flow::Frame;
use lockscreen::palette::Palette;
use tokio::sync::mpsc::UnboundedSender;

use super::moment::Moment;
use crate::layout::{inline, setting_row_content, settings_card_titled, settings_divider, spread};
use crate::state::Store;
use crate::util::bridge::bridge;
use crate::util::worker::{mailbox, spawn_worker};
use backdrop::Backdrop;
use compose::{CORNER_RADIUS, Clock, Look, compose};
use editor::{EditMode, Grab};
use playback::{Mode, Playback};
use sprites::SpriteSet;

/// What the preview lays out for when no monitor has reported its size.
const FALLBACK_OUTPUT: [f32; 2] = [1920.0, 1080.0];
/// How long the bare desktop shows after an unlock before the lock returns.
const UNLOCKED_HOLD: Duration = Duration::from_secs(1);

/// The preview's handle: its state, and the way to ask it to play.
#[derive(Clone)]
pub struct Preview {
    state: State,
    requests: UnboundedSender<Request>,
}

enum Request {
    Play(Moment),
    Show(Mode),
}

#[derive(Clone, Copy)]
struct State {
    store: Store,
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
    edit_mode: Signal<EditMode>,
    grab: Signal<Option<Grab>>,
    /// The instant changing digits are drawn at.
    morph_clock: Signal<Instant>,
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
            size: cx.signal(Size::ZERO),
            device_scale: cx.scale_factor(),
            sprites: cx.signal(SpriteSet::default()),
            backdrop: cx.signal(Backdrop::default()),
            frame: cx.signal(playback.frame(now)),
            playback: cx.signal(playback),
            mode: cx.signal(Mode::Clock),
            animating: cx.signal(false),
            unreadable: cx.signal(false),
            edit_mode: cx.signal(EditMode::default()),
            grab: cx.signal(None),
            morph_clock: cx.signal(now),
        };
        let commands = spawn_worker(cx, studio::run, move |cx, event| state.accept(cx, event));
        state.follow(cx, commands);
        let requests = mailbox(cx, move |cx, request| state.handle(cx, request));
        Self { state, requests }
    }

    /// Whether the lock's own picture failed to load.
    pub fn picture_unreadable(&self) -> Signal<bool> {
        self.state.unreadable
    }

    /// A button reading `caption` that plays `moment` in the preview.
    pub fn play_button(&self, moment: Moment, caption: &'static str) -> impl View + use<> {
        let requests = self.requests.clone();
        button(caption)
            .leading_icon(icons::PLAY)
            .control_size(ButtonSize::Small)
            .label(format!("Play {}", moment.title()))
            .on_click(move |_, _| {
                let _ = requests.send(Request::Play(moment));
            })
    }

    pub fn card(&self, cx: &mut Cx) -> impl View + use<> {
        let state = self.state;
        let requests = self.requests.clone();
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
                    let _ = requests.send(Request::Show(*mode));
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
            .label("Lock screen preview")
            .on_drag_start(move |cx, event| state.grab(cx, event.position))
            .on_drag(move |cx, event| state.drag_by(cx, event.total))
            .on_drag_end(move |cx, _| state.release(cx));
        let edit_mode = bridge(
            cx,
            move |runtime| {
                let mode = state.edit_mode.get(runtime);
                EditMode::ALL
                    .iter()
                    .position(|candidate| *candidate == mode)
                    .unwrap_or_default()
            },
            move |runtime, index: usize| {
                if let Some(mode) = EditMode::ALL.get(index) {
                    state.edit_mode.set(runtime, *mode);
                }
            },
        );
        let plays: Vec<_> = Moment::ALL
            .into_iter()
            .map(|moment| self.play_button(moment, moment.title()))
            .collect();
        settings_card_titled(
            "Preview",
            (
                setting_row_content(picture),
                settings_divider(),
                setting_row_content(spread((
                    toggle_group(Mode::ALL.map(|mode| ToggleItem::text(mode.title())), mode)
                        .label("Show"),
                    spacer(),
                    inline(plays),
                ))),
                settings_divider(),
                setting_row_content(spread((
                    toggle_group(
                        EditMode::ALL.map(|mode| ToggleItem::text(mode.title())),
                        edit_mode,
                    )
                    .label("Clock handles"),
                    spacer(),
                ))),
            ),
        )
    }
}

impl State {
    fn paint(self, runtime: &mut Runtime, size: Size, list: &mut DrawList) {
        if self
            .size
            .with_untracked(runtime, |current| *current != size)
        {
            self.size.set(runtime, size);
        }
        if size.width <= 0.0 || size.height <= 0.0 {
            return;
        }
        let frame = self.frame.get(runtime);
        let output = self.output(runtime);
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
        let look = Look {
            own_background: background != LockBackground::Desktop,
            fill,
            dim,
        };
        let shown = self.shown_frame(runtime);
        let edit = self.clock_edit(runtime, shown);
        let handles = (self.mode.get(runtime) == Mode::Clock && !self.animating.get(runtime))
            .then(|| self.edit_mode.get(runtime));
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
