//! The Input page: everything that turns a person into text.
//!
//! Today that is dictation and nothing else, which is why the page is one card.
//! It is filed as *Input* rather than as *Dictation* for the same reason
//! `input.ron` is — the page is the home for text entry generally, and the
//! second thing to land in it (key repeat, an input method) should be a second
//! card rather than a second page in the sidebar.
//!
//! Everything on it writes straight to `input.ron`, and crowndictator follows
//! that file live: the toggle releases its keyboard grab, the shortcut re-arms
//! it, and the device is picked up at the start of the next recording. There is
//! nothing to apply and nothing to restart.

use blinc_icons::icons;
use crownos_config::schema::input;
use crownuikit::widgets::{Tone, status};
use xilem::WidgetView;

use crate::controls::{auto_name_choice, keybind, switch};
use crate::layout::{
    setting_row_content, setting_row_desc, setting_row_icon, settings_card_titled, settings_divider,
};
use crate::pages::{PageDescriptor, PageView, page};
use crate::state::Store;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Input",
    icon: icons::MIC,
    build,
};

fn build(store: &Store) -> PageView {
    // Everything below the switch configures a feature that is off, so it is
    // shown greyed rather than hidden: a user turning dictation back on should
    // be able to see what it is about to do before it does it.
    let enabled = store.get(input::DictationEnabled);

    let mut rows: Vec<PageView> = vec![
        setting_row_desc(
            "Dictation",
            "Hold the shortcut anywhere to speak, release to type what you said",
            switch(store, input::DictationEnabled),
        )
        .boxed(),
        settings_divider().boxed(),
        setting_row_icon(
            icons::MIC,
            "Microphone source",
            auto_name_choice(
                store,
                input::DictationMicrophone,
                automatic_label(store.microphones.default.as_deref()),
                &store.microphones.names,
            )
            .disabled(!enabled),
        )
        .boxed(),
        settings_divider().boxed(),
        setting_row_desc(
            "Shortcut",
            "Click, then press the keys you want to hold",
            keybind(store, input::DictationHotkey).disabled(!enabled),
        )
        .boxed(),
    ];

    // A shortcut can be cleared, and a cleared one leaves the feature switched
    // on with no way to reach it — worth saying out loud, because the rest of
    // the card looks perfectly healthy.
    if enabled && store.get(input::DictationHotkey).is_empty() {
        rows.push(settings_divider().boxed());
        rows.push(
            setting_row_content(
                status("No shortcut is set, so dictation cannot be started").tone(Tone::Warning),
            )
            .boxed(),
        );
    }

    // Same for a microphone that has since been unplugged: the name is still in
    // the file and still what the user asked for, so it is not silently
    // rewritten — but recording will fall back to the default until it is back.
    if enabled
        && let Some(missing) = missing_microphone(
            store.get(input::DictationMicrophone),
            &store.microphones.names,
        )
    {
        rows.push(settings_divider().boxed());
        rows.push(
            setting_row_content(
                status(format!(
                    "“{missing}” isn't connected — recording uses the default microphone until it is"
                ))
                .tone(Tone::Warning),
            )
            .boxed(),
        );
    }

    rows.push(settings_divider().boxed());
    rows.push(
        setting_row_desc(
            "Use GPU acceleration",
            "Falls back to the processor when no supported GPU is available",
            switch(store, input::DictationGpu).disabled(!enabled),
        )
        .boxed(),
    );

    page(&PAGE, (settings_card_titled("Dictation", rows),))
}

/// What the "Automatic" row says, named with the device it currently resolves
/// to — the way macOS writes "Automatic (MacBook Air Microphone)".
///
/// Falls back to the bare word on a machine that reports no default, where
/// naming nothing in brackets would read as a bug.
fn automatic_label(default: Option<&str>) -> String {
    match default {
        Some(name) => format!("Automatic ({name})"),
        None => "Automatic".to_owned(),
    }
}

/// The configured microphone's name, if one is configured and the machine
/// doesn't currently have it.
///
/// `None` while dictation is on the system default, or while the chosen device
/// is present — and also on a machine that could not enumerate anything at all,
/// where every name would look missing and the warning would be noise.
///
/// Takes the two values rather than the [`Store`] so the rule can be asserted
/// without one: setting a key on a real store writes a real RON file, which a
/// unit test has no business doing to the machine it runs on — the same
/// constraint [`crate::state`]'s own tests work under.
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

    /// The warning fires for a device that is configured and gone, and stays
    /// quiet for every other shape of the same two facts.
    #[test]
    fn only_a_configured_and_absent_microphone_is_reported_missing() {
        // Nothing configured: the default is doing its job.
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
            "a machine that enumerated nothing reports nothing missing, \
             rather than reporting everything missing"
        );
    }
}
