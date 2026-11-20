//! The Web Browser page: which browser CrownOS installs and opens links with.

use crownos_config::system::Browser;
use crownui::kit::icons;
use crownui::prelude::{AnyView, Cx, Semantic, bind};

use super::view::SetupView;
use crate::controls::{Options, field_choice};
use crate::layout::{setting_row_desc, settings_card};
use crate::net::setup::{self, browser_line};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;
use crate::util::worker::mailbox;

static BROWSERS: Options<Browser> = Options::new(&[
    (Browser::Brave.name(), Browser::Brave),
    (Browser::Chrome.name(), Browser::Chrome),
    (Browser::Chromium.name(), Browser::Chromium),
    (Browser::Firefox.name(), Browser::Firefox),
]);

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Web Browser",
    slug: "browser",
    icon: icons::GLOBE,
    keywords: &[
        "chrome", "brave", "firefox", "chromium", "default", "web", "browser",
    ],
    build,
};

fn build(cx: &mut Cx, _store: Store) -> AnyView {
    let setup = SetupView::open(cx);
    let requests = mailbox(cx, move |cx, browser: Browser| {
        setup.apply(cx, setup::set_browser(browser));
    });
    let browser_choice = field_choice(
        cx,
        &BROWSERS,
        move |runtime| setup.config(runtime).browser,
        move |_, browser| {
            let _ = requests.send(browser);
        },
    )
    .disabled(bind(move |runtime| setup.is_locked(runtime)));

    page(
        &PAGE,
        (
            settings_card((setting_row_desc(
                "Default browser",
                bind(move |runtime| setup.line(runtime, browser_line)),
                browser_choice,
            ),)),
            setup.notice(),
            setup.failure(),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_browser_is_offered_in_its_own_order() {
        assert!(BROWSERS.values().eq(Browser::ALL));
    }
}
