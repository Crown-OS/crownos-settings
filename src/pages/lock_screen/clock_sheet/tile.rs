//! One choice on the Clock & colour sheet: a picture on a tile, ringed in
//! the accent while the clock wears it, as ColorOS rings the one in use.

use crownos_config::schema::{ClockLook, Lock};
use crownui::kit::use_theme;
use crownui::prelude::{
    CursorIcon, Listeners, Role, Runtime, Semantic, Styled, TRANSPARENT, View, bind, component,
    vstack,
};

use crate::pages::lock_screen::clock_look::edit;
use crate::state::Store;
use crate::util::theme::tint;

const RING_WIDTH: f32 = 2.0;
const TILE_RADIUS: f32 = 12.0;
/// The picture's room inside the ring.
const TILE_PADDING: f32 = 6.0;
/// What assistive technology hears for the choice the clock wears.
const IN_USE: &str = "In use";

/// `picture` on a tile named `name`, ringed while `worn` holds, putting
/// itself on with `wear`.
pub fn tile(
    store: Store,
    picture: impl View + 'static,
    name: String,
    worn: impl Fn(&ClockLook) -> bool + Copy + 'static,
    wear: impl Fn(&mut ClockLook) + 'static,
) -> impl View {
    component(move |cx| {
        let theme = use_theme(cx);
        let is_worn =
            move |runtime: &mut Runtime| store.section(runtime, |lock: &Lock| worn(&lock.clock));
        vstack(picture)
            .w_full()
            .p(TILE_PADDING)
            .rounded(TILE_RADIUS)
            .bg(tint(theme, |theme| theme.surface.sunken))
            .border(RING_WIDTH)
            .border_color(bind(move |runtime| {
                let worn = is_worn(runtime);
                theme.with(runtime, |theme| match worn {
                    true => theme.accent.end,
                    false => TRANSPARENT,
                })
            }))
            .cursor(CursorIcon::Pointer)
            .role(Role::Button)
            .label(name)
            .value(bind(move |runtime| {
                let shown = if is_worn(runtime) { IN_USE } else { "" };
                shown.to_owned()
            }))
            .focusable(true)
            .on_click(move |cx, _| edit(cx, store, &wear))
    })
}
