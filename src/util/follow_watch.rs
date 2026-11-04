//! A `tokio::sync::watch` channel mirrored into a `Signal`.

use crownui::prelude::{Cx, Signal};
use tokio::sync::watch;

/// Sets `signal` to `convert` of what `receiver` holds now and of every later
/// value, for as long as the calling component lives and the sender does.
pub fn follow_watch<T, S, F>(
    cx: &mut Cx,
    receiver: watch::Receiver<T>,
    signal: Signal<S>,
    convert: F,
) where
    T: Send + Sync + 'static,
    S: 'static,
    F: Fn(&T) -> S + Copy + 'static,
{
    signal.set(cx, convert(&receiver.borrow()));
    follow_changes(cx, receiver, signal, convert);
}

fn follow_changes<T, S, F>(
    cx: &mut Cx,
    mut receiver: watch::Receiver<T>,
    signal: Signal<S>,
    convert: F,
) where
    T: Send + Sync + 'static,
    S: 'static,
    F: Fn(&T) -> S + Copy + 'static,
{
    let next_change = async move {
        let open = receiver.changed().await.is_ok();
        (receiver, open)
    };
    cx.spawn(next_change, move |cx, (receiver, open)| {
        if open {
            signal.set(cx, convert(&receiver.borrow()));
            follow_changes(cx, receiver, signal, convert);
        }
    });
}
