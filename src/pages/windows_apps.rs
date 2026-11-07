//! The Windows Apps page: run .exe and .msi programs with Wine.

use std::time::Duration;

use crownui::kit::{icons, toggle};
use crownui::prelude::{AnyView, Cx, Signal, bind};

use crate::layout::{setting_row_desc, settings_card};
use crate::net::windows_apps::{self, WindowsApps};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;
use crate::util::bridge::bridge;
use crate::util::worker::mailbox;

/// How often an install in progress is looked at again while the page is open.
const RECHECK_INTERVAL: Duration = Duration::from_secs(5);

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Windows Apps",
    slug: "windows-apps",
    icon: icons::APP_WINDOW,
    keywords: &["wine", "exe", "msi", "compatibility"],
    build,
};

fn build(cx: &mut Cx, _store: Store) -> AnyView {
    let status = cx.signal(WindowsApps::current());
    let requests = mailbox(cx, move |cx, on: bool| {
        cx.spawn(windows_apps::request(on), move |cx, now| {
            status.set(cx, now);
        });
    });
    let on = bridge(
        cx,
        move |runtime| status.get(runtime).is_on(),
        move |_, on| {
            let _ = requests.send(on);
        },
    );
    recheck_while_installing(cx, status);

    page(
        &PAGE,
        (settings_card((setting_row_desc(
            "Windows apps support",
            bind(move |runtime| description(status.get(runtime)).to_owned()),
            toggle(on),
        ),)),),
    )
}

fn recheck_while_installing(cx: &mut Cx, status: Signal<WindowsApps>) {
    let pause = async { tokio::time::sleep(RECHECK_INTERVAL).await };
    cx.spawn(pause, move |cx, ()| {
        if status.with_untracked(cx, |status| *status == WindowsApps::Installing) {
            let now = WindowsApps::current();
            if now != WindowsApps::Installing {
                status.set(cx, now);
            }
        }
        recheck_while_installing(cx, status);
    });
}

fn description(status: WindowsApps) -> &'static str {
    match status {
        WindowsApps::Off => "Run .exe and .msi programs with Wine",
        WindowsApps::Installing => "Setting up in the background; Wine installs once you're online",
        WindowsApps::Ready => "Ready. Open any .exe or .msi file to run it",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_state_says_what_happens_next() {
        let lines = [
            WindowsApps::Off,
            WindowsApps::Installing,
            WindowsApps::Ready,
        ]
        .map(description);
        assert!(lines.iter().all(|line| !line.is_empty()));
        assert!(lines[1].contains("online"));
    }
}
