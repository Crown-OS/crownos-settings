//! `slide_transition` gives the first pane no slide-in. This entrance gives
//! the notes card its motion when Settings opens directly on `updates`.

use crownui::ext::Kind;
use crownui::prelude::{Element, Spring, Styled};

pub const OFFSET_Y: f32 = 12.0;
pub const SCALE: f32 = 0.97;
pub const OPACITY: f32 = 0.0;

pub fn spring() -> Spring {
    Spring::smooth()
}

pub fn entering<K: Kind>(element: Element<K>) -> Element<K> {
    element
        .enter(|style| style.opacity(OPACITY).translate_y(OFFSET_Y).scale(SCALE))
        .transition(spring())
}
