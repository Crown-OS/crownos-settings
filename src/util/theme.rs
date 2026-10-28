//! Colors that follow the kit's theme as it changes.

use crownui::kit::Theme;
use crownui::prelude::{Color, IntoProp, Signal, bind};

/// A color property bound to one slot of `theme`.
pub fn tint(
    theme: Signal<Theme>,
    pick: impl Fn(&Theme) -> Color + 'static,
) -> impl IntoProp<Color> {
    bind(move |runtime| theme.with(runtime, &pick))
}
