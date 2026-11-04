//! The Lock Screen page: a live preview drawn by the lock screen's own code,
//! and the settings it follows.

mod background;
mod behaviour;
mod clock;
mod clock_look;
mod looks;
mod moment;
mod preview;
mod prompt;
mod transitions;

use crownui::kit::icons;
use crownui::prelude::{AnyView, Cx};

use crate::pages::{PageDescriptor, page};
use crate::state::Store;
use moment::Moment;
use preview::Preview;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Lock Screen",
    slug: "lock-screen",
    icon: icons::LOCK,
    keywords: &[],
    build,
};

fn build(cx: &mut Cx, store: Store) -> AnyView {
    let preview = Preview::start(cx, store);
    page(
        &PAGE,
        (
            preview.card(cx),
            looks::looks_card(store, &preview),
            clock::clock_card(cx, store, &preview),
            background::background_card(cx, store, &preview),
            prompt::prompt_card(cx, store),
            Moment::ALL
                .into_iter()
                .map(|moment| transitions::transition_card(cx, store, &preview, moment))
                .collect::<Vec<_>>(),
            behaviour::behaviour_card(cx, store),
        ),
    )
}
