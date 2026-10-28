use crownui::kit::{TextInputKind, text_input};
use crownui::prelude::{Cx, Element, Listeners, Runtime};

use crate::util::bridge::derived;

/// A text field showing what `read` derives, that hands its text to `commit`
/// once it is committed — Enter, or leaving the field — rather than on every
/// keystroke.
pub fn field_text(
    cx: &mut Cx,
    read: impl FnMut(&mut Runtime) -> String + 'static,
    mut commit: impl FnMut(&mut Runtime, String) + 'static,
) -> Element<TextInputKind> {
    let draft = derived(cx, read);
    text_input(draft).on_change(move |cx, _| {
        let typed = draft.with(cx, Clone::clone);
        commit(cx, typed);
    })
}
