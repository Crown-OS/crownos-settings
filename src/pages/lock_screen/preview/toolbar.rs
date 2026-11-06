//! The bar under the preview, after the tabs of ColorOS's lock screen
//! editor: which sheet the card opens — Clock & colour, Wallpaper or
//! Effects — and the depth switch, with how far the cut has got.

use crownui::kit::{ToggleItem, toggle, toggle_group};
use crownui::prelude::{Cx, Runtime, Semantic, Signal, View, bind, spacer};

use super::studio::DepthState;
use super::{Preview, State};
use crate::layout::{body_text, inline, paragraph, spread};
use crate::pages::lock_screen::clock_look::look_value;
use crate::util::bridge::bridge;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sheet {
    #[default]
    Clock,
    Wallpaper,
    Effects,
}

impl Sheet {
    pub const ALL: [Self; 3] = [Self::Clock, Self::Wallpaper, Self::Effects];

    pub fn title(self) -> &'static str {
        match self {
            Self::Clock => "Clock & colour",
            Self::Wallpaper => "Wallpaper",
            Self::Effects => "Effects",
        }
    }
}

impl DepthState {
    /// What the depth switch says beside it.
    fn note(self) -> &'static str {
        match self {
            Self::Off | Self::Ready => "",
            Self::NoPicture => "Needs a picture",
            Self::Cutting => "Cutting…",
            Self::HeldBack => "Would hide the clock",
            Self::Unavailable => "Unavailable",
        }
    }
}

impl Preview {
    /// The sheet the card shows under the preview.
    pub fn sheet(&self) -> Signal<Sheet> {
        self.state.sheet
    }

    pub(super) fn toolbar(&self, cx: &mut Cx) -> impl View + use<> {
        let state = self.state;
        let sheet = bridge(
            cx,
            move |runtime| index_of(runtime, state),
            move |runtime, index: usize| {
                if let Some(sheet) = Sheet::ALL.get(index) {
                    state.sheet.set(runtime, *sheet);
                }
            },
        );
        let depth = look_value(
            cx,
            state.store,
            |look| look.depth,
            |look, on| look.depth = on,
        );
        spread((
            toggle_group(
                Sheet::ALL.map(|sheet| ToggleItem::text(sheet.title())),
                sheet,
            )
            .label("Edit"),
            spacer(),
            inline((
                paragraph(bind(move |runtime| {
                    state.depth.get(runtime).note().to_owned()
                })),
                body_text("Depth"),
                toggle(depth).label("Depth"),
            )),
        ))
    }
}

fn index_of(runtime: &mut Runtime, state: State) -> usize {
    let sheet = state.sheet.get(runtime);
    Sheet::ALL
        .iter()
        .position(|candidate| *candidate == sheet)
        .unwrap_or_default()
}
