//! The async half: one loop that serves the page and follows the daemon.

use crownos_ipc::Error as IpcError;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{Instant, sleep_until};

use super::daemon::Daemon;
use super::removal::remove;
use super::{AndroidCommand, AndroidEvent};
use crate::util::worker::MessageProxy;

/// What woke the loop. Holds no borrows, so the handling gets the whole
/// daemon back.
enum Wake {
    Command(Option<AndroidCommand>),
    Inbound(Result<(), IpcError>),
    Retry,
}

/// Returns only when the command channel closes, i.e. with the window.
pub(super) async fn run(
    proxy: MessageProxy<AndroidEvent>,
    mut commands: UnboundedReceiver<AndroidCommand>,
) {
    let mut daemon = Daemon::new(proxy.clone());
    loop {
        let wake = match daemon.client.as_mut() {
            Some(client) => tokio::select! {
                command = commands.recv() => Wake::Command(command),
                inbound = client.pump() => Wake::Inbound(inbound),
            },
            None => tokio::select! {
                command = commands.recv() => Wake::Command(command),
                () = wait_until(daemon.retry_at) => Wake::Retry,
            },
        };
        match wake {
            Wake::Command(None) => return,
            Wake::Command(Some(AndroidCommand::Refresh)) => daemon.refresh().await,
            Wake::Retry => daemon.ask_status().await,
            Wake::Command(Some(AndroidCommand::Stop)) => daemon.stop().await,
            Wake::Command(Some(AndroidCommand::Remove(removal))) => {
                let removed = remove(removal).await;
                if proxy.message(AndroidEvent::Removed(removed)).is_err() {
                    return;
                }
                daemon.refresh().await;
            }
            Wake::Inbound(Ok(())) => daemon.forward_events().await,
            Wake::Inbound(Err(_)) => daemon.unreachable(),
        }
    }
}

/// Sleep until `deadline`, or forever when there is nothing pending.
async fn wait_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}
