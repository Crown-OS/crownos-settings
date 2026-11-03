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

/// `color` with its translucency, `1 - alpha`, multiplied by `factor`.
pub fn with_translucency_factor(color: Color, factor: f32) -> Color {
    Color {
        a: 1.0 - (1.0 - color.a) * factor,
        ..color
    }
}

#[cfg(test)]
mod tests {
    use crownui::prelude::Color;

    use super::with_translucency_factor;

    #[test]
    fn halving_translucency_moves_alpha_halfway_to_opaque() {
        let surface = Color::new(0.1, 0.2, 0.3, 0.8);
        let chrome = with_translucency_factor(surface, 0.5);
        assert_eq!((chrome.r, chrome.g, chrome.b), (0.1, 0.2, 0.3));
        assert!((chrome.a - 0.9).abs() < f32::EPSILON);
    }
}
