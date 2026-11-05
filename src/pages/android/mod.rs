//! The Android apps page: whether Android runs and how, Google Play's
//! registration, the space it takes, and taking it off the machine again.
//!
//! The status is crown-androidd's own answer, plus the setup service's
//! progress while the first-boot setup runs. Opening the page is what makes
//! the worker connect; Android itself starts only when an app is opened.

mod about;
mod google_play;
mod parts;
mod removal;
mod running;
mod status;

use crownui::kit::icons;
use crownui::prelude::{AnyView, Cx, Runtime};

use crate::layout::{optional, when};
use crate::net::android::AndroidState;
use crate::pages::{PageDescriptor, page};
use crate::state::Store;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Android apps",
    slug: "android",
    icon: icons::SMARTPHONE,
    keywords: &["waydroid", "google play", "apk", "phone apps"],
    build,
};

fn build(cx: &mut Cx, store: Store) -> AnyView {
    let android = store.android;
    android.with_untracked(cx, AndroidState::refresh);
    let installed = move |runtime: &mut Runtime| android.with(runtime, AndroidState::is_installed);
    let google_play = move |runtime: &mut Runtime| {
        android.with(runtime, |state| {
            state.is_installed()
                && state
                    .installed
                    .as_ref()
                    .is_some_and(|config| config.google_play)
        })
    };

    page(
        &PAGE,
        (
            optional(
                move |runtime| android.with(runtime, |state| state.error.clone()),
                move |_, message| parts::error_card(android, message),
            ),
            status::card(cx, android),
            when(installed, move |cx| running::card(cx, store)),
            when(google_play, move |_| google_play::card(android)),
            when(installed, move |_| about::card(android)),
            when(installed, move |_| about::storage_card(android)),
            when(installed, move |cx| removal::card(cx, android)),
        ),
    )
}
