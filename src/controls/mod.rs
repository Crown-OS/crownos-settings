//! Setting controls that persist themselves.
//!
//! Every builder here takes the store and a [`Key`](crownos_config::Key), and
//! hands back a kit widget already wired to [`Store::set`](crate::state::Store::set).
//! A page says which field a control edits and never says how — or when — that
//! field reaches the disk.
//!
//! The key is the whole binding: the section to write is `K::Section`, the
//! value type the widget deals in is `K::Value`, and reading the current value
//! is `store.get(rt, key)`. A control therefore cannot read one field and write
//! another, and pointing a switch at an `f64` field is a compile error.
//!
//! Each returns the kit element itself, so a page can still chain
//! `.disabled(..)` onto it.

mod choice;
mod keybind;
mod options;
mod slider;
mod switch;
mod text;

pub use choice::{auto_name_choice, choice, field_choice, name_choice};
pub use keybind::keybind;
pub use options::Options;
pub use slider::{fraction_slider, percent_slider};
pub use switch::switch;
pub use text::field_text;
