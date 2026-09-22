//! The Keybinds page: shortcuts the desktop grabs globally.
//!
//! What belongs here is a shortcut that has to work no matter what has focus.
//! A shortcut that only means something while one feature is running stays with
//! that feature instead — dictation's push-to-talk chord is on the Input page,
//! because it is meaningless with dictation switched off.
//!
//! Everything on it writes straight to `keybinds.ron`.
//!
//! # The compositor is the consumer, and it does not read this yet
//!
//! Only the compositor can honour a global chord: every other CrownOS app is a
//! Wayland client and sees keys just while it is focused, which is the state a
//! launcher shortcut has to work from *outside*. crownpositor still takes its
//! shortcuts from cosmic's own store and has not been pointed at this file, so
//! rebinding here is recorded but not yet acted on. The page is deliberately
//! silent about that: it is a wiring gap on the way to being closed, not
//! something the user can do anything about, and a warning that has to be
//! deleted later is worse than a note in the source.

use blinc_icons::icons;
use crownos_config::schema::keybinds;
use crownuikit::widgets::{Tone, status};
use xilem::WidgetView;

use crate::controls::keybind;
use crate::layout::{
    setting_row_content, setting_row_desc, settings_card_titled, settings_divider,
};
use crate::pages::{PageDescriptor, PageView, page};
use crate::state::Store;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Keybinds",
    icon: icons::KEYBOARD,
    build,
};

fn build(store: &Store) -> PageView {
    let mut rows: Vec<PageView> = vec![
        setting_row_desc(
            "Show the launchpad",
            "Press it once to open, again to close",
            keybind(store, keybinds::Launcher),
        )
        .boxed(),
    ];

    // The same courtesy the Input page pays a cleared dictation shortcut: the
    // row above looks perfectly healthy when it is empty, so the consequence
    // has to be said out loud.
    if store.get(keybinds::Launcher).is_empty() {
        rows.push(settings_divider().boxed());
        rows.push(
            setting_row_content(
                status("No shortcut is set, so the launchpad can only be opened from the bar")
                    .tone(Tone::Warning),
            )
            .boxed(),
        );
    }

    page(&PAGE, (settings_card_titled("Launchpad", rows),))
}
