//! The Lock Screen page: a live preview drawn by the lock screen's own code
//! and edited in place, the sheets it opens, and the settings it follows.

mod background;
mod behaviour;
mod clock;
mod clock_look;
mod clock_sheet;
mod effects;
mod preview;
mod prompt;

use crownui::kit::icons;
use crownui::prelude::{AnyView, Cx, View, keyed};

use crate::pages::{PageDescriptor, page};
use crate::state::Store;
use preview::{Preview, Sheet};

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Lock Screen",
    slug: "lock-screen",
    icon: icons::LOCK,
    keywords: &["clock", "wallpaper", "depth", "password"],
    build,
};

fn build(cx: &mut Cx, store: Store) -> AnyView {
    let preview = Preview::start(cx, store);
    page(
        &PAGE,
        (
            editor(cx, store, &preview),
            clock::clock_card(cx, store),
            prompt::prompt_card(cx, store),
            behaviour::behaviour_card(cx, store),
        ),
    )
}

/// The preview card with the sheet its bar has open under it.
fn editor(cx: &mut Cx, store: Store, preview: &Preview) -> impl View + use<> {
    let (open, shown) = (preview.sheet(), preview.clone());
    let sheet = keyed(
        move |runtime| open.get(runtime),
        move |cx, sheet| match sheet {
            Sheet::Clock => clock_sheet::clock_sheet(cx, store, &shown).boxed(),
            Sheet::Wallpaper => background::wallpaper_sheet(cx, store, &shown).boxed(),
            Sheet::Effects => effects::effects_sheet(cx, store, &shown).boxed(),
        },
    );
    preview.card(cx, sheet)
}
