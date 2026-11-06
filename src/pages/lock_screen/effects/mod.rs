//! The Effects sheet: what dragging the clock in the preview does, the
//! clock's bend and the shadow under it.

mod warp;

use crownui::kit::{ToggleItem, toggle_group};
use crownui::prelude::{Cx, Semantic, View, vstack};

use super::clock_look::look_choice;
use super::preview::{EditMode, Preview};
use crate::controls::{Options, apart, exact};
use crate::layout::{setting_row, setting_row_desc, settings_divider};
use crate::state::Store;
use crate::util::bridge::bridge;
use warp::{WARPS, Warp};

static SHADOWS: Options<f32> = Options::new(&[("None", 0.0), ("Soft", 0.35), ("Strong", 0.8)]);

pub fn effects_sheet(cx: &mut Cx, store: Store, preview: &Preview) -> impl View + use<> {
    let edit_mode = preview.edit_mode();
    let handles = bridge(
        cx,
        move |runtime| {
            let mode = edit_mode.get(runtime);
            EditMode::ALL
                .iter()
                .position(|candidate| *candidate == mode)
                .unwrap_or_default()
        },
        move |runtime, index: usize| {
            if let Some(mode) = EditMode::ALL.get(index) {
                edit_mode.set(runtime, *mode);
            }
        },
    );
    let warp = look_choice(
        cx,
        store,
        &WARPS,
        exact,
        |look| Warp::nearest(look.frame.warp),
        |look, warp| look.frame.warp = warp.corners(),
    );
    let shadow = look_choice(
        cx,
        store,
        &SHADOWS,
        apart,
        |look| look.shadow,
        |look, shadow| look.shadow = shadow,
    );
    vstack((
        setting_row_desc(
            "Drag in the preview to",
            "Move the clock and stretch it by its corner, bend its corners, or move the hours and minutes apart; Ctrl and scroll resize it",
            toggle_group(
                EditMode::ALL.map(|mode| ToggleItem::text(mode.title())),
                handles,
            )
            .label("Drag in the preview to"),
        ),
        settings_divider(),
        setting_row_desc(
            "Warp",
            "Bends the clock; drag its corners in the preview for any other",
            warp.label("Warp"),
        ),
        settings_divider(),
        setting_row("Shadow", shadow.label("Shadow")),
    ))
}
