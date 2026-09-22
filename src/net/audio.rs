//! Which microphones this machine has.
//!
//! A fact, like the Wi-Fi networks in range, and it belongs here for the same
//! reason: `input.ron` records *which* microphone the user chose, and this
//! records which ones exist to choose from.
//!
//! # Why there is no worker here
//!
//! Every other backend under [`crate::net`] is a long-lived future with a
//! command channel, because it is watching something that changes while the
//! window is open and can be asked to act on it. This one is neither. There is
//! nothing to command — the Input page picks a name and writes it to a file —
//! and enumerating devices is a handful of synchronous calls into the audio
//! host that finish in single-digit milliseconds. Spending a task, a channel and
//! an event enum on that would be machinery standing in for a function call.
//!
//! The cost is that a microphone plugged in while the window is open does not
//! appear until it is reopened. That is the honest trade for now; when it starts
//! to bite, the fix is a real worker in the shape [`crate::net`] describes, and
//! the page above does not change either way — it reads a list of names.
//!
//! # Why cpal, and not `pactl` or D-Bus
//!
//! The name in `input.ron` has to be one that [crowndictator] can open, and
//! crowndictator opens its microphone through `cpal`. A device list assembled
//! from anywhere else would be a list of names that look right in the settings
//! panel and match nothing at record time.
//!
//! [crowndictator]: https://github.com/crown-os/crowndictator

use cpal::traits::{DeviceTrait, HostTrait};

/// Every input device the default audio host can name, deduplicated and in the
/// order the host reports them.
///
/// Best effort by design: a machine with no sound server, or one whose host
/// fails to enumerate, gets an empty list and an Input page that offers only
/// "Automatic" — which is a working configuration, not an error state, so
/// nothing here is worth interrupting the user over.
pub fn microphones() -> Vec<String> {
    let host = cpal::default_host();
    let Ok(devices) = host.input_devices() else {
        log_failure("could not enumerate audio input devices");
        return Vec::new();
    };

    let mut names: Vec<String> = Vec::new();
    for device in devices {
        // A device whose name cannot be read cannot be written to `input.ron`
        // either, so it is skipped rather than listed as something blank.
        let Ok(name) = device.name() else { continue };
        // ALSA in particular reports the same card under several descriptions;
        // two identical rows in the picker would be two rows that do the same
        // thing.
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The default input device's name, for labelling the "Automatic" choice with
/// what it currently resolves to — the way macOS writes
/// "Automatic (MacBook Air Microphone)".
///
/// `None` when there is no default, which is what an empty
/// [`microphones`] list also means.
pub fn default_microphone() -> Option<String> {
    cpal::default_host()
        .default_input_device()
        .and_then(|device| device.name().ok())
}

/// Enumeration failing is worth a line on stderr and nothing more — see
/// [`microphones`].
fn log_failure(message: &str) {
    eprintln!("crownsettings: {message}");
}
