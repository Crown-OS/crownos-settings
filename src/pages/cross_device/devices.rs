//! The paired devices, one row each, and the way to add another.

use crownui::kit::{ButtonOptions, ButtonVariant, icon_muted, icons};
use crownui::prelude::{Cx, Signal, Styled, View, bind, for_each, keyed, vstack};

use super::describe::{battery_glyph, battery_text, class_glyph, connection_summary};
use super::parts::{acting, row_button};
use crate::layout::{
    Listed, glyph, hairline_unless_first, inline, listed, optional, setting_row_content,
    setting_row_icon_desc, settings_card_titled, settings_divider, value_text,
};
use crate::net::crownconnect::{CrossDeviceState, Device};
use crate::util::bridge::derived;

const BATTERY_GLYPH: f32 = 15.0;

type Row = Signal<Listed<Device>>;

pub fn card(cx: &mut Cx, state: Signal<CrossDeviceState>) -> impl View + use<> {
    let trusted = derived(cx, move |runtime| {
        state.with(runtime, |state| listed(state.trusted().cloned()))
    });
    settings_card_titled(
        "My Devices",
        (
            for_each(
                trusted,
                |row: &Listed<Device>| row.item.info.id,
                move |cx, row| device_row(cx, state, row),
            ),
            settings_divider(),
            setting_row_content(acting(
                row_button("Add Another Device…", ButtonVariant::Secondary)
                    .leading_icon(icons::PLUS),
                state,
                CrossDeviceState::begin_pairing,
            )),
        ),
    )
}

fn device_row(cx: &mut Cx, state: Signal<CrossDeviceState>, row: Row) -> impl View + use<> {
    let (id, name) = row.with_untracked(cx, |row| (row.item.info.id, row.item.info.name.clone()));

    let battery = optional(
        move |runtime| row.with(runtime, |row| row.item.info.battery),
        |_, battery| {
            inline((
                icon_muted(battery_glyph(battery)).size(BATTERY_GLYPH),
                value_text(battery_text(battery)),
            ))
        },
    );
    let open = acting(
        row_button("Options…", ButtonVariant::Ghost),
        state,
        move |state| state.open(id),
    );

    vstack((
        hairline_unless_first(row),
        setting_row_icon_desc(
            keyed(
                move |runtime| row.with(runtime, |row| class_glyph(row.item.info.class)),
                |_, body| glyph(body),
            ),
            name,
            bind(move |runtime| {
                row.with(runtime, |row| connection_summary(&row.item.info).to_owned())
            }),
            inline((battery, open)),
        ),
    ))
}
