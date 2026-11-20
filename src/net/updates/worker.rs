//! The async half: one loop that runs the verbs and watches the daemon.

use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{Instant, sleep_until};
use update_client::ClientError;

use super::daemon::Daemon;
use super::{RestartKind, UpdatesCommand, UpdatesEvent};
use crate::net::session::restart_session;
use crate::util::worker::MessageProxy;

enum Wake {
    Command(Option<UpdatesCommand>),
    Inbound(Result<(), ClientError>),
    Retry,
}

/// Returns only when the command channel closes, i.e. with the window.
pub(super) async fn run(
    proxy: MessageProxy<UpdatesEvent>,
    mut commands: UnboundedReceiver<UpdatesCommand>,
) {
    let mut daemon = Daemon::new(proxy);

    loop {
        daemon.reconnect_when_due().await;
        daemon.forward_events();

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
            Wake::Command(Some(command)) => execute(&mut daemon, command).await,
            Wake::Inbound(Err(_)) => daemon.lost(),
            Wake::Inbound(Ok(())) | Wake::Retry => {}
        }
    }
}

async fn execute(daemon: &mut Daemon, command: UpdatesCommand) {
    let Some(client) = daemon.client().await else {
        return;
    };
    let accepted: Result<(), ClientError> = match command {
        UpdatesCommand::Check => client.check().await,
        UpdatesCommand::Download => client.download().await,
        UpdatesCommand::Apply => client.apply().await,
        UpdatesCommand::Restart(_) => client.restart().await,
    };
    match accepted {
        Ok(()) if command == UpdatesCommand::Restart(RestartKind::Session) => {
            if let Err(error) = restart_session().await {
                daemon.say(UpdatesEvent::SessionRestartFailed(format!("{error:#}")));
            }
        }
        Ok(()) => {}
        Err(error) => daemon.report(error),
    }
}

/// Sleep until `deadline`, or forever when there is nothing pending.
async fn wait_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}
