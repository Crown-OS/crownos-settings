//! A plugin's declarative settings, drawn with the app's own controls.
//!
//! Every control reads the daemon's resolved value through
//! [`PluginsState::value`] and writes through [`PluginsState::set_setting`],
//! which refuses anything the field would not accept.

use crownos_config::schema::theme::COLOR_TOKENS;
use crownui::kit::{
    SliderOptions, TextInputKind, TextInputOptions, select, slider, text_input, toggle, use_theme,
};
use crownui::prelude::{
    AnyView, Cx, Element, Listeners, Runtime, Semantic, Signal, Styled, View, bind, component,
    vstack,
};

use crate::layout::{
    inline, setting_row, setting_row_desc, settings_card, settings_divider, value_text,
};
use crate::net::plugins::{Field, PluginsState, SettingValue};
use crate::util::bridge::{bridge, derived};

/// The widest a text or path field grows.
const TEXT_WIDTH: f32 = 260.0;
/// Same as the app's other sliders.
const SLIDER_WIDTH: f32 = 320.0;
const SWATCH_SIZE: f32 = 16.0;
const SWATCH_RADIUS: f32 = 4.0;

/// The card of fields one plugin declares.
pub fn fields_card(
    cx: &mut Cx,
    state: Signal<PluginsState>,
    id: &str,
    fields: &[Field],
) -> AnyView {
    let rows: Vec<AnyView> = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let row = field_row(cx, state, id, field);
            if index == 0 {
                row
            } else {
                vstack((settings_divider(), row)).boxed()
            }
        })
        .collect();
    settings_card(rows).boxed()
}

fn field_row(cx: &mut Cx, state: Signal<PluginsState>, id: &str, field: &Field) -> AnyView {
    let binding = Binding {
        state,
        id: id.to_owned(),
        key: field.key().to_owned(),
    };
    let control = match field {
        Field::Toggle { label, .. } => toggle(binding.toggle(cx)).label(label.clone()).boxed(),
        Field::Slider {
            label,
            min,
            max,
            step,
            unit,
            ..
        } => binding
            .slider(cx, (*min, *max, *step), unit.clone(), label.clone())
            .boxed(),
        Field::Select { label, options, .. } => {
            let labels = cx.signal(options.iter().map(|option| option.label.clone()).collect());
            let values: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
            select(labels, binding.choice(cx, values))
                .label(label.clone())
                .boxed()
        }
        Field::ColorToken { label, .. } => binding.color_token(cx, label.clone()).boxed(),
        Field::Text { label, secret, .. } => {
            let input = binding.text(cx).label(label.clone());
            if *secret {
                input.secret().boxed()
            } else {
                input.boxed()
            }
        }
        Field::File { label, filter, .. } => binding
            .text(cx)
            .placeholder(file_placeholder(filter))
            .label(label.clone())
            .boxed(),
    };
    let description = field_description(field);
    if description.is_empty() {
        setting_row(field.label(), control).boxed()
    } else {
        setting_row_desc(field.label().to_owned(), description.to_owned(), control).boxed()
    }
}

/// One field of one plugin, as the controls read and write it.
#[derive(Clone)]
struct Binding {
    state: Signal<PluginsState>,
    id: String,
    key: String,
}

impl Binding {
    fn read(&self, runtime: &mut Runtime) -> Option<SettingValue> {
        self.state
            .with(runtime, |state| state.value(&self.id, &self.key))
    }

    fn write(&self, runtime: &mut Runtime, value: SettingValue) {
        self.state.update(runtime, |state| {
            state.set_setting(&self.id, &self.key, value)
        });
    }

    fn toggle(self, cx: &mut Cx) -> Signal<bool> {
        let reader = self.clone();
        bridge(
            cx,
            move |runtime| reader.read(runtime).is_some_and(|value| as_bool(&value)),
            move |runtime, on| self.write(runtime, SettingValue::Bool(on)),
        )
    }

    fn slider(
        self,
        cx: &mut Cx,
        (min, max, step): (f64, f64, f64),
        unit: String,
        label: String,
    ) -> impl View + use<> {
        let reader = self.clone();
        let value = bridge(
            cx,
            move |runtime| reader.read(runtime).map_or(min, |value| as_number(&value)) as f32,
            move |runtime, shown| self.write(runtime, SettingValue::Number(snapped(shown))),
        );
        inline((
            slider(value)
                .range(min as f32, max as f32)
                .step(step as f32)
                .basis(SLIDER_WIDTH)
                .shrink(100.0)
                .label(label),
            value_text(bind(move |runtime| {
                amount(f64::from(value.get(runtime)), &unit)
            })),
        ))
    }

    fn choice(self, cx: &mut Cx, values: Vec<String>) -> Signal<Option<usize>> {
        let reader = self.clone();
        let readable = values.clone();
        bridge(
            cx,
            move |runtime| {
                reader
                    .read(runtime)
                    .and_then(|value| index_of(&readable, &value))
            },
            move |runtime, index: Option<usize>| {
                if let Some(value) = index.and_then(|index| values.get(index)) {
                    self.write(runtime, SettingValue::Text(value.clone()));
                }
            },
        )
    }

    fn color_token(self, cx: &mut Cx, label: String) -> impl View + use<> {
        let labels = cx.signal(
            COLOR_TOKENS
                .iter()
                .map(|token| (*token).to_owned())
                .collect(),
        );
        let tokens: Vec<String> = COLOR_TOKENS
            .iter()
            .map(|token| (*token).to_owned())
            .collect();
        let selected = self.choice(cx, tokens);
        inline((swatch(selected), select(labels, selected).label(label)))
    }

    /// A field that is sent when it is committed — Enter or leaving it — not
    /// on every keystroke.
    fn text(self, cx: &mut Cx) -> Element<TextInputKind> {
        let reader = self.clone();
        let stored = cx.memo(move |runtime| {
            reader
                .read(runtime)
                .map(|value| as_text(&value))
                .unwrap_or_default()
        });
        let draft = derived(cx, move |runtime| stored.get(runtime));
        text_input(draft).width(TEXT_WIDTH).on_change(move |cx, _| {
            let typed = draft.with(cx, Clone::clone);
            self.write(cx, SettingValue::Text(typed));
        })
    }
}

/// The chosen token's color in the current theme.
fn swatch(selected: Signal<Option<usize>>) -> impl View {
    component(move |cx| {
        let theme = use_theme(cx);
        vstack(())
            .size(SWATCH_SIZE)
            .rounded(SWATCH_RADIUS)
            .bg(bind(move |runtime| {
                let token = selected
                    .get(runtime)
                    .and_then(|index| COLOR_TOKENS.get(index).copied());
                theme.with(runtime, |theme| {
                    token
                        .and_then(|token| theme.token(token))
                        .unwrap_or(theme.surface.border)
                })
            }))
    })
}

fn field_description(field: &Field) -> &str {
    match field {
        Field::Toggle { description, .. }
        | Field::Slider { description, .. }
        | Field::Select { description, .. }
        | Field::ColorToken { description, .. }
        | Field::Text { description, .. }
        | Field::File { description, .. } => description,
    }
}

fn as_bool(value: &SettingValue) -> bool {
    matches!(value, SettingValue::Bool(true))
}

fn as_number(value: &SettingValue) -> f64 {
    match value {
        SettingValue::Number(number) => *number,
        _ => 0.0,
    }
}

fn as_text(value: &SettingValue) -> String {
    match value {
        SettingValue::Text(text) => text.clone(),
        _ => String::new(),
    }
}

fn index_of(values: &[String], value: &SettingValue) -> Option<usize> {
    let SettingValue::Text(text) = value else {
        return None;
    };
    values.iter().position(|candidate| candidate == text)
}

/// A slider's `f32` back in the `f64` the field stores, without the binary
/// noise the narrowing added (`0.3` rather than `0.30000001192`).
fn snapped(shown: f32) -> f64 {
    (f64::from(shown) * 1e4).round() / 1e4
}

/// `value` as a slider's readout: at most two decimals, then the unit.
fn amount(value: f64, unit: &str) -> String {
    format!("{}{unit}", (value * 100.0).round() / 100.0)
}

fn file_placeholder(filter: &[String]) -> String {
    if filter.is_empty() {
        "Path to a file".to_owned()
    } else {
        let patterns: Vec<String> = filter
            .iter()
            .map(|extension| format!("*.{extension}"))
            .collect();
        patterns.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_map_onto_the_controls_that_show_them() {
        assert!(as_bool(&SettingValue::Bool(true)));
        assert!(!as_bool(&SettingValue::Text("true".into())));
        assert_eq!(as_number(&SettingValue::Number(2.5)), 2.5);
        assert_eq!(as_number(&SettingValue::Bool(true)), 0.0);
        assert_eq!(as_text(&SettingValue::Text("hi".into())), "hi");

        let options = vec!["fast".to_owned(), "slow".to_owned()];
        assert_eq!(
            index_of(&options, &SettingValue::Text("slow".into())),
            Some(1)
        );
        assert_eq!(index_of(&options, &SettingValue::Text("gone".into())), None);
        assert_eq!(index_of(&options, &SettingValue::Number(1.0)), None);
    }

    #[test]
    fn every_color_token_is_offered_and_found() {
        let tokens: Vec<String> = COLOR_TOKENS
            .iter()
            .map(|token| (*token).to_owned())
            .collect();
        assert_eq!(
            index_of(&tokens, &SettingValue::Text("accent.start".into())),
            Some(0)
        );
    }

    #[test]
    fn slider_readouts_are_short() {
        assert_eq!(snapped(0.3_f32), 0.3);
        assert_eq!(amount(2.0, "s"), "2s");
        assert_eq!(amount(0.333_333, "%"), "0.33%");
        assert_eq!(
            file_placeholder(&["png".into(), "jpg".into()]),
            "*.png, *.jpg"
        );
    }
}
