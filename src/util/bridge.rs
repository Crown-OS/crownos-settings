//! Two-way links between the state a page reads and the `Signal`s the kit's
//! widgets edit.

use crownui::prelude::{Cx, Runtime, Signal};

/// A signal holding what `read` derives, for a widget that edits a `Signal`.
///
/// Every edit the user makes that differs from the source is handed to
/// `write`; every change the source reports flows back into the signal, so a
/// control snaps back when the system refuses an edit, exactly like a view
/// rebuilt from state would.
pub fn bridge<T: Clone + PartialEq + 'static>(
    cx: &mut Cx,
    mut read: impl FnMut(&mut Runtime) -> T + 'static,
    mut write: impl FnMut(&mut Runtime, T) + 'static,
) -> Signal<T> {
    let initial = cx.untrack(|runtime| read(runtime));
    let known = cx.store(initial.clone());
    let local = cx.signal(initial);
    cx.effect(move |runtime| {
        let value = read(runtime);
        if local.with_untracked(runtime, |current| *current != value) {
            local.set(runtime, value.clone());
        }
        known.update(runtime, |known| *known = value);
    });
    cx.effect(move |runtime| {
        let value = local.get(runtime);
        if known.with(runtime, |known| *known != value) {
            write(runtime, value);
        }
    });
    local
}

/// A read-only signal following what `read` derives, for widgets that take
/// their content as a `Signal`.
pub fn derived<T: Clone + PartialEq + 'static>(
    cx: &mut Cx,
    read: impl FnMut(&mut Runtime) -> T + 'static,
) -> Signal<T> {
    bridge(cx, read, |_, _| {})
}
