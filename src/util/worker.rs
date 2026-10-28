//! Long-lived backend futures on CrownUI's task runtime, talking to the UI
//! thread through two channels: commands down, events up.

use std::future::Future;

use crownui::prelude::Cx;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// Events handed to the UI thread per wake-up, at most. A burst of D-Bus
/// signals folds into one reactive flush instead of one per signal.
const BURST: usize = 64;

/// The worker's half of the event channel. Cheap to clone.
pub struct MessageProxy<E>(UnboundedSender<E>);

impl<E> Clone for MessageProxy<E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<E> From<UnboundedSender<E>> for MessageProxy<E> {
    fn from(sender: UnboundedSender<E>) -> Self {
        Self(sender)
    }
}

impl<E> MessageProxy<E> {
    /// Posts `event` to the UI thread; fails only once the window is gone.
    pub fn message(&self, event: E) -> Result<(), SendError<E>> {
        self.0.send(event)
    }
}

/// Starts `run` for as long as the calling component lives and calls
/// `on_event` on the UI thread for everything it posts. The returned sender is
/// the command channel `run` receives from.
pub fn spawn_worker<C, E, F>(
    cx: &mut Cx,
    run: impl FnOnce(MessageProxy<E>, UnboundedReceiver<C>) -> F,
    on_event: impl FnMut(&mut Cx, E) + 'static,
) -> UnboundedSender<C>
where
    C: Send + 'static,
    E: Send + 'static,
    F: Future<Output = ()> + Send + 'static,
{
    let (commands, command_receiver) = unbounded_channel();
    let (events, event_receiver) = unbounded_channel();
    cx.spawn(run(MessageProxy(events), command_receiver), |_, ()| {});
    pump(cx, event_receiver, on_event);
    commands
}

/// A channel whose messages are handled on the UI thread, in the calling
/// component's scope, for as long as it lives: how a handler that runs
/// outside that scope hands it work that must die with it.
pub fn mailbox<E: Send + 'static>(
    cx: &mut Cx,
    on_message: impl FnMut(&mut Cx, E) + 'static,
) -> UnboundedSender<E> {
    let (sender, receiver) = unbounded_channel();
    pump(cx, receiver, on_message);
    sender
}

fn pump<E: Send + 'static>(
    cx: &mut Cx,
    mut events: UnboundedReceiver<E>,
    mut on_event: impl FnMut(&mut Cx, E) + 'static,
) {
    let next_burst = async move {
        let mut burst = Vec::new();
        let open = events.recv_many(&mut burst, BURST).await > 0;
        (events, burst, open)
    };
    cx.spawn(next_burst, move |cx, (events, burst, open)| {
        for event in burst {
            on_event(cx, event);
        }
        if open {
            pump(cx, events, on_event);
        }
    });
}
