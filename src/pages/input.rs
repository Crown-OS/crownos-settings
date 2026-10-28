//! The Input page: everything that turns a person into text.
//!
//! Today that is dictation and nothing else, which is why the page is one card.
//! It is filed as *Input* rather than as *Dictation* for the same reason
//! `input.ron` is — the page is the home for text entry generally.
//!
//! Everything on it writes straight to `input.ron`, and crowndictator follows
//! that file live: the toggle releases its keyboard grab, the shortcut re-arms
//! it, and the device is picked up at the start of the next recording.

use crownos_config::schema::input;
use crownui::kit::{StatusOptions, Tone, icons, status};
use crownui::prelude::{AnyView, Cx, Runtime, Semantic, bind, vstack};

use crate::controls::{auto_name_choice, keybind, switch};
use crate::layout::{
    glyph, optional, setting_row_content, setting_row_desc, setting_row_icon, settings_card_titled,
    settings_divider, when,
};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;
use crate::util::bridge::derived;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Input",
    slug: "input",
    icon: icons::MIC,
    build,
};

fn build(cx: &mut Cx, store: Store) -> AnyView {
    // Everything below the switch configures a feature that is off, so it is
    // shown disabled rather than hidden: a user turning dictation back on
    // should be able to see what it is about to do before it does it.
    let enabled = move |runtime: &mut Runtime| store.get(runtime, input::DictationEnabled);
    let disabled = move || bind(move |runtime| !enabled(runtime));
    let names = derived(cx, move |runtime| {
        store
            .microphones
            .with(runtime, |microphones| microphones.names.clone())
    });

    let microphone = auto_name_choice(
        cx,
        store,
        input::DictationMicrophone,
        names,
        move |runtime| {
            store.microphones.with(runtime, |microphones| {
                automatic_label(microphones.default.as_deref())
            })
        },
    )
    .disabled(disabled());

    page(
        &PAGE,
        (settings_card_titled(
            "Dictation",
            (
                setting_row_desc(
                    "Dictation",
                    "Hold the shortcut anywhere to speak, release to type what you said",
                    switch(cx, store, input::DictationEnabled),
                ),
                settings_divider(),
                setting_row_icon(glyph(icons::MIC), "Microphone source", microphone),
                settings_divider(),
                setting_row_desc(
                    "Shortcut",
                    "Click, then press the keys you want to hold",
                    keybind(cx, store, input::DictationHotkey).disabled(disabled()),
                ),
                // A cleared shortcut leaves the feature on with no way to
                // reach it — worth saying, because the rest of the card looks
                // perfectly healthy.
                when(
                    move |runtime| {
                        enabled(runtime) && store.get(runtime, input::DictationHotkey).is_empty()
                    },
                    |_| warning("No shortcut is set, so dictation cannot be started".to_owned()),
                ),
                // A microphone that has since been unplugged is still what the
                // user asked for, so it is not silently rewritten — but
                // recording falls back to the default until it is back.
                optional(
                    move |runtime| {
                        if !enabled(runtime) {
                            return None;
                        }
                        let chosen = store.get(runtime, input::DictationMicrophone);
                        store.microphones.with(runtime, |microphones| {
                            missing_microphone(chosen, &microphones.names)
                        })
                    },
                    |_, missing| {
                        warning(format!(
                            "“{missing}” isn't connected — recording uses the default microphone until it is"
                        ))
                    },
                ),
                settings_divider(),
                setting_row_desc(
                    "Use GPU acceleration",
                    "Falls back to the processor when no supported GPU is available",
                    switch(cx, store, input::DictationGpu).disabled(disabled()),
                ),
            ),
        ),),
    )
}

fn warning(message: String) -> impl crownui::prelude::View {
    vstack((
        settings_divider(),
        setting_row_content(status(message).tone(Tone::Warning)),
    ))
}

/// What the "Automatic" row says, named with the device it currently resolves
/// to — the way macOS writes "Automatic (MacBook Air Microphone)".
fn automatic_label(default: Option<&str>) -> String {
    match default {
        Some(name) => format!("Automatic ({name})"),
        None => "Automatic".to_owned(),
    }
}

/// The configured microphone's name, if one is configured and the machine
/// doesn't currently have it — and nothing on a machine that could not
/// enumerate anything at all, where every name would look missing.
fn missing_microphone(chosen: Option<String>, present: &[String]) -> Option<String> {
    let chosen = chosen?;
    if present.is_empty() || present.contains(&chosen) {
        return None;
    }
    Some(chosen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn devices() -> Vec<String> {
        vec!["Yeti".to_owned(), "Webcam".to_owned()]
    }

    #[test]
    fn automatic_names_the_device_it_resolves_to() {
        assert_eq!(automatic_label(Some("Yeti")), "Automatic (Yeti)");
        assert_eq!(
            automatic_label(None),
            "Automatic",
            "a machine with no default should not offer empty brackets"
        );
    }

    #[test]
    fn only_a_configured_and_absent_microphone_is_reported_missing() {
        assert_eq!(missing_microphone(None, &devices()), None);
        assert_eq!(
            missing_microphone(Some("Webcam".to_owned()), &devices()),
            None,
            "it is plugged in"
        );
        assert_eq!(
            missing_microphone(Some("Yeti Pro".to_owned()), &devices()),
            Some("Yeti Pro".to_owned())
        );
        assert_eq!(
            missing_microphone(Some("Yeti Pro".to_owned()), &[]),
            None,
            "a machine that enumerated nothing reports nothing missing"
        );
    }
}
