//! Windows apps support: whether Wine is wanted, and whether it has arrived.
//!
//! The switch only asks. `crownos-windows-apps.service` installs Wine as soon
//! as the machine is online and retries until it is, so the page reads the
//! outcome from the filesystem rather than following the install.

use std::path::Path;
use std::process::Command;

/// Present while the service is enabled, which is what "on" means.
const ENABLED_LINK: &str =
    "/etc/systemd/system/multi-user.target.wants/crownos-windows-apps.service";
const WINE: &str = "/usr/bin/wine";
/// Run through `pkexec`; its polkit action is `org.crownos.windows-apps.manage`.
const HELPER: &str = "/usr/bin/crownos-windows-apps";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsApps {
    Off,
    Installing,
    Ready,
}

impl WindowsApps {
    pub fn current() -> Self {
        match (Path::new(ENABLED_LINK).exists(), Path::new(WINE).exists()) {
            (false, _) => Self::Off,
            (true, false) => Self::Installing,
            (true, true) => Self::Ready,
        }
    }

    pub fn is_on(self) -> bool {
        self != Self::Off
    }
}

/// Asks for Windows apps support on or off, and returns where that left it:
/// unchanged when the user dismisses the password prompt.
pub async fn request(on: bool) -> WindowsApps {
    let action = if on { "on" } else { "off" };
    let _ =
        tokio::task::spawn_blocking(move || Command::new("pkexec").args([HELPER, action]).status())
            .await;
    WindowsApps::current()
}
