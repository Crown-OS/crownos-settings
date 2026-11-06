//! Writing signals without waking what reads them for nothing.

use crownui::prelude::{Runtime, Signal};

/// Sets `signal` to `value` unless it already holds it, so effects that read
/// it only run when it really changes.
pub fn set_if_changed<T: PartialEq + 'static>(runtime: &mut Runtime, signal: Signal<T>, value: T) {
    if signal.with_untracked(runtime, |current| *current != value) {
        signal.set(runtime, value);
    }
}
