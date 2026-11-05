//! The status card: what Android is doing, read from the daemon's answer and
//! the setup service's record, and the one control that acts on it.
//!
//! There is no Start control: the daemon offers no call that starts Android
//! without launching an app, and opening any Android app starts it.

use std::borrow::Cow;

use crownui::kit::{ButtonVariant, icons, progress};
use crownui::prelude::{Cx, Runtime, Semantic, Signal, Styled, View, bind, vstack};

use super::parts::{acting, row_button};
use crate::layout::{
    glyph, optional, setting_row_content, setting_row_desc, setting_row_icon_desc,
    settings_card_titled, settings_divider, value_text, when,
};
use crate::net::android::{AndroidService, AndroidState, RuntimeState, SetupStatus, Stage};
use crate::util::bridge::derived;

const UNSUPPORTED: &str = "This computer can't run Android apps.";
const GETTING_READY: &str = "Getting Android ready";
const PERCENT: f32 = 100.0;

/// What the status row says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Checking,
    NotInstalled,
    /// Set up, but the daemon is not answering.
    NotRunning,
    SettingUp {
        percent: Option<u8>,
        message: String,
    },
    SetupFailed {
        reason: String,
    },
    Starting,
    Stopping,
    Stopped,
    Running {
        windows: u32,
    },
    /// Running with stock windowing because the CrownOS layer did not load.
    SafeMode,
    Unsupported {
        reason: String,
    },
}

impl Status {
    pub fn label(&self) -> Cow<'static, str> {
        match self {
            Self::Checking => "Checking…".into(),
            Self::NotInstalled => "Not installed".into(),
            Self::NotRunning => "Not running".into(),
            Self::SettingUp {
                percent: Some(percent),
                ..
            } => format!("Setting up · {percent}%").into(),
            Self::SettingUp { percent: None, .. } => "Setting up…".into(),
            Self::SetupFailed { .. } => "Setup failed".into(),
            Self::Starting => "Starting…".into(),
            Self::Stopping => "Stopping…".into(),
            Self::Stopped => "Stopped".into(),
            Self::Running { windows: 1 } => "Running · 1 window".into(),
            Self::Running { windows } => format!("Running · {windows} windows").into(),
            Self::SafeMode => "Safe mode".into(),
            Self::Unsupported { .. } => "Not supported".into(),
        }
    }

    /// One line on what the status means for the user.
    pub fn detail(&self) -> &str {
        match self {
            Self::Checking => "Asking the Android service",
            Self::NotInstalled => "Android apps aren't set up on this computer",
            Self::NotRunning => "The Android service isn't answering",
            Self::SettingUp { message, .. } => message,
            Self::SetupFailed { reason } | Self::Unsupported { reason } => reason,
            Self::Starting => "Android apps open as soon as it's ready",
            Self::Stopping => "Closing every Android app",
            Self::Stopped => "Starts when you open an Android app",
            Self::Running { .. } => "Android apps are open in their own windows",
            Self::SafeMode => "Android apps share one window because the CrownOS layer didn't load",
        }
    }

    pub fn can_stop(&self) -> bool {
        matches!(self, Self::Starting | Self::Running { .. } | Self::SafeMode)
    }

    fn percent(&self) -> Option<u8> {
        match self {
            Self::SettingUp { percent, .. } => *percent,
            _ => None,
        }
    }
}

/// The status, from what the daemon said and the setup service wrote.
pub fn status_of(state: &AndroidState) -> Status {
    let runtime = match &state.service {
        AndroidService::Checking => return Status::Checking,
        AndroidService::NotInstalled => return Status::NotInstalled,
        AndroidService::NotRunning => return Status::NotRunning,
        AndroidService::Answered(runtime) => runtime,
    };
    match runtime.state {
        RuntimeState::NotInstalled => Status::NotInstalled,
        RuntimeState::SettingUp => setting_up(state.setup.as_ref()),
        RuntimeState::Stopped => Status::Stopped,
        RuntimeState::Starting => Status::Starting,
        RuntimeState::Running => Status::Running {
            windows: runtime.windows,
        },
        RuntimeState::Stopping => Status::Stopping,
        RuntimeState::SafeMode => Status::SafeMode,
        RuntimeState::Unsupported => Status::Unsupported {
            reason: state
                .setup
                .as_ref()
                .and_then(|setup| setup.failure.as_ref())
                .map_or_else(|| UNSUPPORTED.to_owned(), |failure| failure.detail.clone()),
        },
    }
}

fn setting_up(setup: Option<&SetupStatus>) -> Status {
    match setup {
        Some(setup) if setup.stage == Stage::Failed => Status::SetupFailed {
            reason: setup.message.clone(),
        },
        Some(setup) => Status::SettingUp {
            percent: setup.percent,
            message: setup.message.clone(),
        },
        None => Status::SettingUp {
            percent: None,
            message: GETTING_READY.to_owned(),
        },
    }
}

pub fn card(cx: &mut Cx, android: Signal<AndroidState>) -> impl View + use<> {
    let status = derived(cx, move |runtime| android.with(runtime, status_of));
    let read = move |runtime: &mut Runtime, pick: fn(&Status) -> String| status.with(runtime, pick);
    settings_card_titled(
        "Android",
        (
            setting_row_icon_desc(
                glyph(icons::SMARTPHONE),
                "Status",
                bind(move |runtime| read(runtime, |status| status.detail().to_owned())),
                value_text(bind(move |runtime| {
                    read(runtime, |status| status.label().into_owned())
                })),
            ),
            optional(
                move |runtime| status.with(runtime, Status::percent),
                |_, percent| progress_row(percent),
            ),
            when(
                move |runtime| android.with(runtime, AndroidState::is_installed),
                move |_| stop_row(android, status),
            ),
        ),
    )
}

fn progress_row(percent: u8) -> impl View {
    setting_row_content(progress(f32::from(percent) / PERCENT).w_full())
}

fn stop_row(android: Signal<AndroidState>, status: Signal<Status>) -> impl View {
    let stop = acting(
        row_button("Stop", ButtonVariant::Secondary),
        android,
        |state| {
            state.stop();
        },
    )
    .disabled(bind(move |runtime| !status.with(runtime, Status::can_stop)));
    vstack((
        settings_divider(),
        setting_row_desc(
            "Stop Android now",
            "Closes every Android app and gives its memory back",
            stop,
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::android::{Failure, FailureCode, RuntimeStatus};

    fn with_service(service: AndroidService) -> AndroidState {
        let mut state = AndroidState::default();
        state.service = service;
        state
    }

    fn answered(state: RuntimeState, windows: u32) -> AndroidState {
        with_service(AndroidService::Answered(RuntimeStatus {
            state,
            windows,
            ..RuntimeStatus::stopped()
        }))
    }

    #[test]
    fn an_unreachable_daemon_is_not_running_or_not_installed() {
        assert_eq!(status_of(&AndroidState::default()), Status::Checking);
        assert_eq!(
            status_of(&with_service(AndroidService::NotRunning)).label(),
            "Not running"
        );
        assert_eq!(
            status_of(&with_service(AndroidService::NotInstalled)).label(),
            "Not installed"
        );
        assert_eq!(
            status_of(&answered(RuntimeState::NotInstalled, 0)),
            Status::NotInstalled
        );
    }

    #[test]
    fn setting_up_shows_the_setup_services_progress() {
        let mut state = answered(RuntimeState::SettingUp, 0);
        assert_eq!(status_of(&state).label(), "Setting up…");
        assert_eq!(status_of(&state).detail(), GETTING_READY);

        state.setup = Some(SetupStatus::new(
            Stage::Downloading,
            Some(42),
            "Downloading Android",
        ));
        let status = status_of(&state);
        assert_eq!(status.label(), "Setting up · 42%");
        assert_eq!(status.detail(), "Downloading Android");
        assert_eq!(status.percent(), Some(42));

        state.setup = Some(SetupStatus::failed(Failure {
            code: FailureCode::Network,
            detail: "The download stopped.".to_owned(),
            retryable: true,
        }));
        assert_eq!(
            status_of(&state),
            Status::SetupFailed {
                reason: "The download stopped.".to_owned()
            }
        );
    }

    #[test]
    fn running_counts_windows_and_can_be_stopped() {
        let one = status_of(&answered(RuntimeState::Running, 1));
        assert_eq!(one.label(), "Running · 1 window");
        assert!(one.can_stop());
        assert_eq!(
            status_of(&answered(RuntimeState::Running, 3)).label(),
            "Running · 3 windows"
        );
        assert!(status_of(&answered(RuntimeState::SafeMode, 1)).can_stop());
        assert!(status_of(&answered(RuntimeState::Starting, 0)).can_stop());
        assert!(!status_of(&answered(RuntimeState::Stopped, 0)).can_stop());
        assert!(!status_of(&answered(RuntimeState::Stopping, 0)).can_stop());
    }

    #[test]
    fn unsupported_says_why_when_setup_found_out() {
        let mut state = answered(RuntimeState::Unsupported, 0);
        assert_eq!(status_of(&state).detail(), UNSUPPORTED);
        state.setup = Some(SetupStatus::failed(Failure {
            code: FailureCode::UnsupportedPageSize,
            detail: "This kernel uses 16 KB pages.".to_owned(),
            retryable: false,
        }));
        let status = status_of(&state);
        assert_eq!(status.label(), "Not supported");
        assert_eq!(status.detail(), "This kernel uses 16 KB pages.");
    }

    #[test]
    fn safe_mode_and_stopped_explain_themselves() {
        assert_eq!(
            status_of(&answered(RuntimeState::SafeMode, 1)).label(),
            "Safe mode"
        );
        assert_eq!(
            status_of(&answered(RuntimeState::Stopped, 0)).detail(),
            "Starts when you open an Android app"
        );
    }
}
