//! The async half: one loop that serves the pages and watches the daemon.

use crownos_ipc::Error as IpcError;
use crownplugin_proto::plugind;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{Instant, sleep_until};

use super::daemon::{Daemon, Failure, NOT_RUNNING};
use super::{PluginsCommand, PluginsEvent};
use crate::util::worker::MessageProxy;

/// What woke the loop. Holds no borrows, so the handling gets the whole
/// daemon back — including the freedom to replace its client.
enum Wake {
    Command(Option<PluginsCommand>),
    Inbound(Result<(), IpcError>),
    Retry,
}

/// Returns only when the command channel closes, i.e. with the window.
pub(super) async fn run(
    proxy: MessageProxy<PluginsEvent>,
    mut commands: UnboundedReceiver<PluginsCommand>,
) {
    let mut daemon = Daemon::new(proxy);

    loop {
        daemon.reconnect_when_due().await;
        while daemon.forward_events() {
            daemon.refresh().await;
        }

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

async fn execute(daemon: &mut Daemon, command: PluginsCommand) {
    match command {
        PluginsCommand::Refresh => daemon.refresh().await,
        PluginsCommand::Install(source) => {
            let event = match daemon.request(&plugind::install { source }).await {
                Ok(id) => PluginsEvent::Installed(id),
                Err(Failure::Refused(message)) => PluginsEvent::InstallFailed(message),
                Err(Failure::Offline) => PluginsEvent::InstallFailed(NOT_RUNNING.to_owned()),
            };
            daemon.say(event);
        }
        PluginsCommand::Uninstall(id) => {
            daemon
                .call(&plugind::uninstall { id }, "Could not uninstall")
                .await;
        }
        PluginsCommand::SetEnabled { id, enabled } => {
            daemon
                .call(
                    &plugind::set_enabled { id, enabled },
                    "Could not switch the plugin",
                )
                .await;
        }
        PluginsCommand::SetTheme(id) => {
            daemon
                .call(&plugind::set_theme { id }, "Could not change the theme")
                .await;
        }
        PluginsCommand::SetGrant {
            id,
            capability,
            decision,
        } => {
            daemon
                .call(
                    &plugind::set_grant {
                        id,
                        capability,
                        decision,
                    },
                    "Could not change the permission",
                )
                .await;
        }
        PluginsCommand::LoadSettings(id) => daemon.load_settings(id).await,
        PluginsCommand::SetSetting { id, key, value } => {
            let request = plugind::set_setting {
                id: id.clone(),
                key,
                value,
            };
            if daemon
                .call(&request, "Could not save the setting")
                .await
                .is_none()
            {
                daemon.load_settings(id).await;
            }
        }
        PluginsCommand::LoadLogs(id) => daemon.load_logs(id).await,
        PluginsCommand::Reload(id) => {
            daemon
                .call(&plugind::reload { id }, "Could not reload")
                .await;
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
