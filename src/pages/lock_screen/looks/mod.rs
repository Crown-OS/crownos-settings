//! Whole looks to pick the clock from, and the faces it can wear: tiles
//! painted by the lock screen's own code, the one in use ringed.

mod preset;
mod warp;

use crownos_config::schema::Lock;
use crownos_config::schema::lock::{ClockKey, ClockStyleKey};
use crownui::kit::{INTER_FAMILY, use_theme};
use crownui::prelude::{
    CursorIcon, Listeners, Role, Semantic, Styled, TRANSPARENT, View, WHITE, bind, component,
    hstack, text, vstack,
};

pub use preset::{FACES, LOOKS};
pub use warp::{WARPS, Warp};

use super::preview::{CORNER_RADIUS, Preview};
use crate::layout::{setting_row_content, settings_card_titled};
use crate::state::Store;
use crate::util::theme::tint;

const COLUMNS: u16 = 4;
const TILE_GAP: f32 = 14.0;
/// The selection ring hugs the picture's edge, as ColorOS draws it.
const RING_WIDTH: f32 = 2.5;
const RING_RADIUS: f32 = CORNER_RADIUS + RING_WIDTH;
const NAME_SIZE: f32 = 13.0;
const NAME_GAP: f32 = 6.0;
/// The "In use" badge, in the picture's top right corner.
const BADGE_SIZE: f32 = 11.0;
const BADGE_INSET: f32 = 6.0;
const BADGE_PADDING: [f32; 2] = [6.0, 2.0];
const BADGE_RADIUS: f32 = 4.0;
const FACE_HEIGHT: f32 = 132.0;

pub fn looks_card(store: Store, preview: &Preview) -> impl View + use<> {
    let tiles: Vec<_> = LOOKS
        .iter()
        .enumerate()
        .map(|(index, look)| {
            tile(
                store,
                preview.look_picture(index),
                look.name,
                move |lock| look.is_worn_by(lock),
                move |lock| look.apply(lock),
            )
        })
        .collect();
    settings_card_titled("Looks", setting_row_content(grid(tiles)))
}

/// The faces the clock can wear, in its current height, weight and light.
pub fn faces(store: Store, preview: &Preview) -> impl View + use<> {
    let tiles: Vec<_> = FACES
        .iter()
        .enumerate()
        .map(|(index, face)| {
            tile(
                store,
                preview.face_picture(index, FACE_HEIGHT),
                face.name,
                move |lock| face.is_worn_by(lock),
                move |lock| face.apply(lock),
            )
        })
        .collect();
    setting_row_content(grid(tiles))
}

fn grid(tiles: Vec<impl View>) -> impl View {
    vstack(tiles)
        .grid()
        .grid_cols(COLUMNS)
        .gap(TILE_GAP)
        .w_full()
}

/// `picture`, ringed while `worn` holds, putting itself on with `apply`.
fn tile(
    store: Store,
    picture: impl View + 'static,
    name: &'static str,
    worn: impl Fn(&Lock) -> bool + Copy + 'static,
    apply: impl Fn(&mut Lock) + 'static,
) -> impl View {
    component(move |cx| {
        let theme = use_theme(cx);
        let is_worn = move |runtime: &mut _| store.section(runtime, worn);
        let badge = hstack(
            text("In use")
                .text_size(BADGE_SIZE)
                .font_family(INTER_FAMILY)
                .font_medium()
                .text_color(WHITE),
        )
        .px(BADGE_PADDING[0])
        .py(BADGE_PADDING[1])
        .rounded(BADGE_RADIUS)
        .bg(tint(theme, |theme| theme.accent.end))
        .absolute()
        .top(BADGE_INSET + RING_WIDTH)
        .right(BADGE_INSET + RING_WIDTH)
        .pointer_events_none()
        .hidden(bind(move |runtime| !is_worn(runtime)));
        let framed = vstack((picture, badge))
            .relative()
            .w_full()
            .p(RING_WIDTH)
            .border(RING_WIDTH)
            .rounded(RING_RADIUS)
            .border_color(bind(move |runtime| {
                let worn = is_worn(runtime);
                theme.with(
                    runtime,
                    |theme| if worn { theme.accent.end } else { TRANSPARENT },
                )
            }));
        vstack((
            framed,
            text(name)
                .text_size(NAME_SIZE)
                .font_family(INTER_FAMILY)
                .text_color(tint(theme, |theme| theme.text.muted))
                .self_center(),
        ))
        .gap(NAME_GAP)
        .items_stretch()
        .w_full()
        .cursor(CursorIcon::Pointer)
        .role(Role::Button)
        .label(name)
        .on_click(move |cx, _| {
            let mut lock = Lock {
                clock_style: store.get(cx, ClockStyleKey),
                clock: store.get(cx, ClockKey),
                ..Lock::default()
            };
            apply(&mut lock);
            store.set(cx, ClockStyleKey, lock.clock_style);
            store.set(cx, ClockKey, lock.clock);
        })
    })
}
