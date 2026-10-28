//! What an entity may do, grouped by how much the user has to be asked.

use crownui::kit::{
    CalloutOptions, StatusOptions, ToggleItem, Tone, callout, icon_colored, icons, status,
    toggle_group,
};
use crownui::prelude::{AnyView, Cx, Semantic, Signal, Styled, View, keyed, vstack};

use crate::layout::{
    card_stack, glyph, nothing, setting_row_content, setting_row_icon_desc, settings_card_titled,
    settings_divider,
};
use crate::net::plugins::{CapabilityInfo, Decision, Level, PluginsState};
use crate::util::bridge::bridge;

const ICON_STROKE: f32 = 2.0;
const ICON_SIZE: f32 = 18.0;

/// The three answers, in the order the segmented control shows them.
const ANSWERS: [(&str, Option<Decision>); 3] = [
    ("Ask", None),
    ("Allow", Some(Decision::Granted)),
    ("Deny", Some(Decision::Denied)),
];

/// What the cards are built from: everything but the decisions, which are
/// bound per row.
#[derive(Debug, Clone, PartialEq)]
struct Asked {
    key: String,
    summary: String,
    level: Level,
}

pub fn cards(state: Signal<PluginsState>, id: String) -> impl View {
    let watched = id.clone();
    keyed(
        move |runtime| {
            state.with(runtime, |state| {
                state.entity(&watched).map_or_else(Vec::new, |entity| {
                    entity.capabilities.iter().map(asked).collect::<Vec<_>>()
                })
            })
        },
        move |cx, capabilities| {
            if capabilities.is_empty() {
                return nothing().boxed();
            }
            card_stack(
                [Level::Safe, Level::Sensitive, Level::Dangerous]
                    .into_iter()
                    .map(|level| level_card(cx, state, &id, &capabilities, level))
                    .collect::<Vec<_>>(),
            )
            .boxed()
        },
    )
}

fn asked(capability: &CapabilityInfo) -> Asked {
    Asked {
        key: capability.key.clone(),
        summary: capability.summary.clone(),
        level: capability.level,
    }
}

fn level_card(
    cx: &mut Cx,
    state: Signal<PluginsState>,
    id: &str,
    capabilities: &[Asked],
    level: Level,
) -> AnyView {
    if capabilities
        .iter()
        .all(|capability| capability.level != level)
    {
        return nothing().boxed();
    }
    let mut rows: Vec<AnyView> = Vec::new();
    if level == Level::Dangerous {
        rows.push(
            setting_row_content(
                callout(Tone::Warning, "These can change or read anything you can")
                    .body("Allow them only for plugins you trust.")
                    .icon(icons::TRIANGLE_ALERT),
            )
            .boxed(),
        );
    }
    for capability in capabilities
        .iter()
        .filter(|capability| capability.level == level)
    {
        if !rows.is_empty() {
            rows.push(settings_divider().boxed());
        }
        rows.push(capability_row(cx, state, id, capability));
    }
    settings_card_titled(level_title(level), rows).boxed()
}

fn level_title(level: Level) -> &'static str {
    match level {
        Level::Safe => "Allowed automatically",
        Level::Sensitive => "Sensitive permissions",
        Level::Dangerous => "Dangerous permissions",
    }
}

fn capability_row(
    cx: &mut Cx,
    state: Signal<PluginsState>,
    id: &str,
    capability: &Asked,
) -> AnyView {
    if capability.level == Level::Safe {
        return setting_row_icon_desc(
            glyph(icons::SHIELD_CHECK),
            capability.summary.clone(),
            capability.key.clone(),
            status("Granted").tone(Tone::Success),
        )
        .boxed();
    }
    let (read_id, read_key) = (id.to_owned(), capability.key.clone());
    let (write_id, write_key) = (id.to_owned(), capability.key.clone());
    let answer = bridge(
        cx,
        move |runtime| {
            state.with(runtime, |state| {
                answer_index(decision_of(state, &read_id, &read_key))
            })
        },
        move |runtime, index: usize| {
            let decision = ANSWERS.get(index).and_then(|(_, decision)| *decision);
            state.update(runtime, |state| {
                state.set_grant(write_id.clone(), write_key.clone(), decision);
            });
        },
    );
    let leading = if capability.level == Level::Dangerous {
        icon_colored(icons::TRIANGLE_ALERT, ICON_STROKE, |theme| {
            theme.status.warning
        })
        .size(ICON_SIZE)
        .boxed()
    } else {
        glyph(icons::SHIELD).boxed()
    };
    setting_row_icon_desc(
        vstack(leading),
        capability.summary.clone(),
        capability.key.clone(),
        toggle_group(
            ANSWERS.iter().map(|(label, _)| ToggleItem::text(*label)),
            answer,
        )
        .label(format!("Permission for {}", capability.summary)),
    )
    .boxed()
}

fn decision_of(state: &PluginsState, id: &str, key: &str) -> Option<Decision> {
    state
        .entity(id)?
        .capabilities
        .iter()
        .find(|capability| capability.key == key)?
        .decision
}

fn answer_index(decision: Option<Decision>) -> usize {
    ANSWERS
        .iter()
        .position(|(_, answer)| *answer == decision)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_decision_has_its_segment() {
        assert_eq!(answer_index(None), 0);
        assert_eq!(answer_index(Some(Decision::Granted)), 1);
        assert_eq!(answer_index(Some(Decision::Denied)), 2);
        for (index, (_, decision)) in ANSWERS.iter().enumerate() {
            assert_eq!(answer_index(*decision), index);
        }
    }
}
