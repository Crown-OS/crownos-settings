//! The System Updates page, over crownos-updated: the installed release, the
//! next one with its notes, and each step from download to restart.

mod cards;
mod entrance;
mod notes;
mod verbs;

use crownui::kit::icons;
use crownui::prelude::{AnyView, Cx};

use crate::layout::{optional, when};
use crate::net::updates::{Link, UpdatesState};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "System Updates",
    slug: "updates",
    icon: icons::REFRESH_CW,
    keywords: &[
        "update", "upgrade", "release", "version", "restart", "rollback",
    ],
    build,
};

fn build(_cx: &mut Cx, store: Store) -> AnyView {
    let updates = store.updates;
    page(
        &PAGE,
        (
            cards::version_card(updates),
            when(
                move |runtime| updates.with(runtime, |state| state.link == Link::Offline),
                |_| cards::offline_card(),
            ),
            optional(
                move |runtime| updates.with(runtime, |state| state.error.clone()),
                move |_, message| cards::error_card(updates, message),
            ),
            optional(
                move |runtime| updates.with(runtime, cards::failure_of),
                move |_, failure| cards::failure_card(updates, failure),
            ),
            optional(
                move |runtime| {
                    updates.with(runtime, |state| {
                        state.snapshot.as_ref()?.pending_restart.clone()
                    })
                },
                move |_, pending| cards::restart_card(updates, pending),
            ),
            optional(
                move |runtime| updates.with(runtime, UpdatesState::running_phase),
                move |_, phase| cards::progress_card(updates, phase),
            ),
            optional(
                move |runtime| updates.with(runtime, cards::ReleaseNotes::of),
                move |_, release| cards::notes_card(updates, release),
            ),
        ),
    )
}
