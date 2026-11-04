//! Which microphones this machine has.
//!
//! A fact, like the Wi-Fi networks in range, and it belongs here for the same
//! reason: `input.ron` records *which* microphone the user chose, and this
//! records which ones exist to choose from.
//!
//! # Why there is no worker here
//!
//! Every other backend under [`crate::net`] is a long-lived future with a
//! command channel, because it can be asked to act on what it watches. This
//! one cannot — the Input page picks a name and writes it to a file — and
//! CrownUI's audio crate already watches the PipeWire graph on a thread of its
//! own, publishing every hotplug and default change on a `watch` channel. So
//! the store mirrors that channel into a signal, and a microphone plugged in
//! while the window is open shows up in the picker straight away.
//!
//! # Why PipeWire node names
//!
//! The name in `input.ron`'s `dictation_microphone` has to be one that
//! [crowndictator] can open, and crowndictator opens its microphone by PipeWire
//! `node.name` through the same audio crate. The picker shows each node's
//! description and stores its node name. crown-voiced's own enhanced
//! `crownos-voice.*` nodes are never listed: the crate leaves them out, as
//! apps reach them by asking for enhancement rather than by picking them.
//!
//! [crowndictator]: https://github.com/crown-os/crowndictator

use audio::SourceInfo;
use crownui::prelude::{Cx, Signal};

use crate::util::follow_watch::follow_watch;

/// One capture device, as the Input page's picker needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Microphone {
    /// The PipeWire `node.name`, which is what `input.ron` stores.
    pub node_name: String,
    /// What the picker shows, such as "Built-in Audio Analog Stereo".
    pub description: String,
}

/// What is plugged in, in the order PipeWire announced it.
///
/// Empty on a machine without PipeWire, which leaves the Input page offering
/// only "Automatic" — a working configuration, not an error state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Microphones {
    pub available: Vec<Microphone>,
    /// The system default's description, for labelling the "Automatic" choice
    /// with what it actually means right now.
    pub default: Option<String>,
}

impl Microphones {
    fn from_sources(sources: &[SourceInfo]) -> Self {
        Self {
            available: sources
                .iter()
                .map(|source| Microphone {
                    node_name: source.id.as_str().to_owned(),
                    description: source.description.clone(),
                })
                .collect(),
            default: sources
                .iter()
                .find(|source| source.is_default)
                .map(|source| source.description.clone()),
        }
    }

    /// Whether a microphone with this node name is plugged in.
    pub fn has(&self, node_name: &str) -> bool {
        self.available
            .iter()
            .any(|microphone| microphone.node_name == node_name)
    }
}

/// Keeps `microphones` in step with the PipeWire graph for as long as the
/// calling component lives.
pub fn follow_microphones(cx: &mut Cx, microphones: Signal<Microphones>) {
    follow_watch(cx, audio::sources(), microphones, |sources: &Vec<_>| {
        Microphones::from_sources(sources)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio::SourceId;

    fn source(node_name: &str, description: &str, is_default: bool) -> SourceInfo {
        SourceInfo {
            id: SourceId::new(node_name),
            description: description.to_owned(),
            is_default,
        }
    }

    #[test]
    fn the_picker_shows_descriptions_and_stores_node_names() {
        let microphones = Microphones::from_sources(&[
            source("alsa_input.pci-analog", "Built-in Audio", false),
            source("alsa_input.usb-yeti", "Yeti Stereo Microphone", true),
        ]);
        assert_eq!(
            microphones.available[1],
            Microphone {
                node_name: "alsa_input.usb-yeti".to_owned(),
                description: "Yeti Stereo Microphone".to_owned(),
            }
        );
        assert_eq!(
            microphones.default.as_deref(),
            Some("Yeti Stereo Microphone")
        );
        assert!(microphones.has("alsa_input.pci-analog"));
        assert!(
            !microphones.has("Built-in Audio"),
            "a description is not a node name"
        );
    }
}
