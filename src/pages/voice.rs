//! The Voice page: AI enhancement of the microphone every app hears.
//!
//! The switch and both choices write `voice.ron`, which crown-voiced follows
//! live. With the switch on, the daemon puts an enhanced copy of the
//! microphone in place as the system default source, so an app that opens the
//! default input is enhanced without knowing it.
//!
//! The status row is the daemon's own answer to `status()`. Asking is also
//! what socket-activates it, which is why the page asks again the moment the
//! switch goes on: the enhanced microphone should exist before the user goes
//! looking for it.

use crownos_config::schema::{VoiceBackend, VoiceEnhancement, voice};
use crownui::kit::icons;
use crownui::prelude::{AnyView, Cx, Semantic, bind};

use crate::controls::{Options, choice, switch_then};
use crate::layout::{
    glyph, setting_row_desc, setting_row_icon, settings_card_titled, settings_divider, value_text,
};
use crate::net::voice::{VoiceService, VoiceState};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;

static ENHANCEMENTS: Options<VoiceEnhancement> = Options::new(&[
    ("Off", VoiceEnhancement::Off),
    ("Light", VoiceEnhancement::Light),
    ("Noise suppression", VoiceEnhancement::NoiseSuppression),
    ("Studio voice", VoiceEnhancement::StudioVoice),
]);

static BACKENDS: Options<VoiceBackend> = Options::new(&[
    ("Automatic", VoiceBackend::Automatic),
    ("CPU only", VoiceBackend::Cpu),
]);

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Voice",
    slug: "voice",
    icon: icons::AUDIO_WAVEFORM,
    keywords: &["noise", "microphone", "enhance"],
    build,
};

fn build(cx: &mut Cx, store: Store) -> AnyView {
    store.voice.with_untracked(cx, VoiceState::refresh);
    let not_enhancing = bind(move |runtime| !store.get(runtime, voice::EnhanceAllApps));
    let enhance_all_apps = switch_then(cx, store, voice::EnhanceAllApps, move |runtime, on| {
        if on {
            store.voice.with_untracked(runtime, VoiceState::refresh);
        }
    });

    page(
        &PAGE,
        (
            settings_card_titled(
                "Microphone",
                (
                    setting_row_desc(
                        "Enhance microphone for all apps",
                        "Apps that record from the default microphone hear the enhanced voice",
                        enhance_all_apps,
                    ),
                    settings_divider(),
                    setting_row_icon(
                        glyph(icons::WAND_SPARKLES),
                        "Enhancement",
                        choice(cx, store, voice::AllAppsEnhancement, &ENHANCEMENTS)
                            .disabled(not_enhancing),
                    ),
                ),
            ),
            settings_card_titled(
                "Processing",
                (
                    setting_row_desc(
                        "Processing hardware",
                        "Automatic picks the best this machine can run",
                        choice(cx, store, voice::Backend, &BACKENDS),
                    ),
                    settings_divider(),
                    setting_row_icon(
                        glyph(icons::CPU),
                        "Running on",
                        value_text(bind(move |runtime| {
                            store
                                .voice
                                .with(runtime, |voice| service_label(&voice.service))
                        })),
                    ),
                ),
            ),
        ),
    )
}

/// What the status row says: the backend in use, or why there is none.
fn service_label(service: &VoiceService) -> String {
    match service {
        VoiceService::Checking => "Checking…".to_owned(),
        VoiceService::Running {
            backend: Some(backend),
        } => backend.clone(),
        VoiceService::Running { backend: None } => "No backend available".to_owned(),
        VoiceService::NotRunning => "Not running".to_owned(),
        VoiceService::NotInstalled => "Not installed".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_row_names_the_backend_or_says_why_there_is_none() {
        let running = |backend: Option<&str>| VoiceService::Running {
            backend: backend.map(str::to_owned),
        };
        assert_eq!(
            service_label(&running(Some("cpu: Ryzen 7 7840U"))),
            "cpu: Ryzen 7 7840U"
        );
        assert_eq!(service_label(&running(None)), "No backend available");
        assert_eq!(service_label(&VoiceService::NotRunning), "Not running");
        assert_eq!(service_label(&VoiceService::NotInstalled), "Not installed");
    }
}
