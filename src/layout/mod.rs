//! This app's page/card/row chrome, composed from the kit's widgets. Knows
//! nothing about state.
//!
//! Only metrics live here. Every color comes from the kit's theme, which is
//! what makes this app follow `~/.config/crownos/appearance.ron`.

mod card;
mod conditional;
mod list;
mod page;
mod row;
mod text;

pub use card::{settings_card, settings_card_titled, settings_divider};
pub use conditional::{nothing, optional, when};
pub use list::{Listed, hairline_unless_first, listed};
pub use page::{card_stack, settings_page};
pub use row::{
    glyph, live_glyph, setting_row, setting_row_content, setting_row_desc, setting_row_icon,
    setting_row_icon_desc,
};
pub use text::{INLINE_GAP, headline, inline, paragraph, spread, value_text};
