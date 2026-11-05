//! Android apps: crown-androidd, the runtime manager, and the root setup
//! service behind it, for the Android apps page.
//!
//! The house shape (see [`crate::net`]): [`AndroidCommand`] down,
//! [`AndroidEvent`] up, [`AndroidState`] in the [`Store`](crate::state::Store).
//! The worker stays idle until the page first asks, then follows the daemon's
//! `RuntimeChanged` and `SetupProgress` for as long as the window lives.
//! Asking for the status only socket-activates the daemon; Android itself
//! starts when an app is launched, never because this page looked.

mod daemon;
mod removal;
mod survey;
mod worker;

#[cfg(test)]
pub use android_protocol::GpuTier;
pub use android_protocol::{RuntimeState, RuntimeStatus, SetupStatus};
pub use crownos_config::system::{AndroidConfig, AndroidTier};
pub use setup_state::Stage;
#[cfg(test)]
pub use setup_state::{Failure, FailureCode};

use crownui::prelude::{Cx, Signal};
use tokio::sync::mpsc::UnboundedSender;

use crate::util::worker::spawn_worker;

/// Everything the page can ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndroidCommand {
    Refresh,
    /// Stop Android now instead of at the idle timeout.
    Stop,
    Remove(Removal),
}

/// The two ways of taking Android off this machine. Both run the setup
/// service's own `remove` as root through `pkexec`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    /// Delete every app and its data, then set Android up again.
    ResetData,
    /// Remove Android apps from this computer.
    Uninstall,
}

/// What the last attempt to reach crown-androidd found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AndroidService {
    /// No answer yet.
    #[default]
    Checking,
    /// The daemon answered with this.
    Answered(RuntimeStatus),
    /// Android apps are set up but nothing answers the daemon's socket.
    NotRunning,
    /// Neither the daemon nor a chosen Android setup is on this machine.
    NotInstalled,
}

/// Disk use, as far as this user can read it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Storage {
    /// The Android system images.
    pub images_bytes: Option<u64>,
    /// Every app's data. Partly owned by Android's own users, so a lower bound.
    pub data_bytes: Option<u64>,
}

/// Everything the worker can tell the page.
#[derive(Debug, Clone, PartialEq)]
pub enum AndroidEvent {
    Service(AndroidService),
    Setup(Option<SetupStatus>),
    /// `/etc/crownos/android.ron`, or `None` when Android apps were never set up.
    Installed(Option<AndroidConfig>),
    Storage(Storage),
    RegistrationId(Option<String>),
    /// The removal in flight finished; `Err` says why it did not.
    Removed(Result<(), String>),
    /// A request failed. Already human-readable; shown as-is.
    Failed(String),
}

/// The Android half of the app's [`Store`](crate::state::Store).
#[derive(Debug, Default)]
pub struct AndroidState {
    sender: Option<UnboundedSender<AndroidCommand>>,
    pub service: AndroidService,
    /// The setup service's latest record.
    pub setup: Option<SetupStatus>,
    pub installed: Option<AndroidConfig>,
    pub storage: Storage,
    /// The Google services framework id, once Android has one.
    pub registration_id: Option<String>,
    /// The removal running as root, if one is.
    pub removing: Option<Removal>,
    /// The last failure, until dismissed.
    pub error: Option<String>,
}

impl AndroidState {
    /// Read everything again. Silent without a worker, as in tests.
    pub fn refresh(&self) {
        self.send(AndroidCommand::Refresh);
    }

    pub fn stop(&self) {
        self.send(AndroidCommand::Stop);
    }

    /// Start `removal`, unless one is already running.
    pub fn remove(&mut self, removal: Removal) {
        if self.removing.is_some() {
            return;
        }
        self.removing = Some(removal);
        self.error = None;
        self.send(AndroidCommand::Remove(removal));
    }

    pub fn dismiss_error(&mut self) {
        self.error = None;
    }

    /// Whether the installer or the setup service left Android apps enabled.
    pub fn is_installed(&self) -> bool {
        self.installed.as_ref().is_some_and(|config| config.enabled)
    }

    fn send(&self, command: AndroidCommand) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(command);
        }
    }

    fn apply(&mut self, event: AndroidEvent) {
        match event {
            AndroidEvent::Service(service) => self.service = service,
            AndroidEvent::Setup(setup) => self.setup = setup,
            AndroidEvent::Installed(installed) => self.installed = installed,
            AndroidEvent::Storage(storage) => self.storage = storage,
            AndroidEvent::RegistrationId(id) => self.registration_id = id,
            AndroidEvent::Removed(result) => {
                self.removing = None;
                self.error = result.err();
            }
            AndroidEvent::Failed(message) => self.error = Some(message),
        }
    }
}

/// Starts the Android backend for as long as the calling component lives.
pub fn android_worker(cx: &mut Cx, android: Signal<AndroidState>) {
    let sender = spawn_worker(cx, worker::run, move |cx, event| {
        android.update(cx, |state| state.apply(event));
    });
    android.update(cx, |state| state.sender = Some(sender));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_removal_runs_once_and_its_failure_stays_until_dismissed() {
        let mut state = AndroidState::default();
        state.remove(Removal::Uninstall);
        state.remove(Removal::ResetData);
        assert_eq!(state.removing, Some(Removal::Uninstall));

        state.apply(AndroidEvent::Removed(Err(
            "Permission was not given.".to_owned()
        )));
        assert_eq!(state.removing, None);
        assert_eq!(state.error.as_deref(), Some("Permission was not given."));
        state.dismiss_error();
        assert_eq!(state.error, None);
    }

    #[test]
    fn only_an_enabled_config_is_installed() {
        let mut state = AndroidState::default();
        assert!(!state.is_installed());
        state.apply(AndroidEvent::Installed(Some(AndroidConfig::default())));
        assert!(!state.is_installed(), "remove leaves the file, disabled");
        state.apply(AndroidEvent::Installed(Some(AndroidConfig {
            enabled: true,
            ..AndroidConfig::default()
        })));
        assert!(state.is_installed());
    }
}
