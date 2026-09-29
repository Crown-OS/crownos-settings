//! The async half: one loop that serves the page and watches the daemon.

use std::time::Duration;

use crownconnect_linux::ipc::proto;
use crownos_ipc::Error as IpcError;
use xilem::core::MessageProxy;
use xilem::tokio::sync::mpsc::UnboundedReceiver;
use xilem::tokio::time::{Instant, sleep_until};

use super::daemon::Daemon;
use super::{CrossDeviceCommand, CrossDeviceEvent, PairingInvite};
use crate::util::clock::unix_millis;
use crate::util::qr::qr_brush;

const TICK: Duration = Duration::from_secs(1);

/// What woke the loop. Holds no borrows, so the handling gets the whole
/// daemon back — including the freedom to replace its client.
enum Wake {
    Command(Option<CrossDeviceCommand>),
    Inbound(Result<(), IpcError>),
    Retry,
    Tick,
}

/// Returns only when the command channel closes, i.e. with the window.
///
/// Commands run inline: each is one local-socket round trip, and pushes that
/// arrive meanwhile queue in the client until [`Daemon::forward_events`].
pub(super) async fn run(
    proxy: MessageProxy<CrossDeviceEvent>,
    mut commands: UnboundedReceiver<CrossDeviceCommand>,
) {
    let mut daemon = Daemon::new(proxy);
    let mut countdown = Countdown::default();

    loop {
        daemon.reconnect_when_due().await;

        let wake = match daemon.client.as_mut() {
            Some(client) => xilem::tokio::select! {
                command = commands.recv() => Wake::Command(command),
                inbound = client.pump() => Wake::Inbound(inbound),
                () = wait_until(countdown.next_tick) => Wake::Tick,
            },
            None => xilem::tokio::select! {
                command = commands.recv() => Wake::Command(command),
                () = wait_until(daemon.retry_at) => Wake::Retry,
                () = wait_until(countdown.next_tick) => Wake::Tick,
            },
        };

        match wake {
            Wake::Command(None) => return,
            Wake::Command(Some(command)) => execute(&mut daemon, &mut countdown, command).await,
            Wake::Inbound(Err(_)) => daemon.lost(),
            Wake::Inbound(Ok(())) | Wake::Retry => {}
            Wake::Tick => countdown.tick(&daemon),
        }
        daemon.forward_events();
    }
}

async fn execute(daemon: &mut Daemon, countdown: &mut Countdown, command: CrossDeviceCommand) {
    match command {
        CrossDeviceCommand::BeginPairing => {
            let Some(offer) = daemon
                .call(&proto::pairing_begin {}, "Could not start pairing")
                .await
            else {
                return;
            };
            match qr_brush(&offer.qr) {
                Ok(qr) => {
                    countdown.start(offer.expires_unix_ms, daemon);
                    daemon.say(CrossDeviceEvent::PairingInvite(PairingInvite {
                        qr,
                        expires_unix_ms: offer.expires_unix_ms,
                    }));
                }
                Err(error) => daemon.say(CrossDeviceEvent::Error(format!(
                    "Could not draw the pairing code: {error}"
                ))),
            }
        }
        CrossDeviceCommand::CancelPairing => {
            countdown.stop();
            daemon.notify(&proto::pairing_cancel {}).await;
        }
        CrossDeviceCommand::ConfirmPairing { id, accept } => {
            daemon
                .call(
                    &proto::pairing_confirm { id, accept },
                    "Could not answer the pairing request",
                )
                .await;
        }
        CrossDeviceCommand::Forget(id) => {
            if daemon
                .call(&proto::forget { id }, "Could not forget the device")
                .await
                .is_some()
            {
                daemon.refresh_devices().await;
            }
        }
        CrossDeviceCommand::SetFeature {
            id,
            feature,
            enabled,
        } => {
            daemon
                .call(
                    &proto::set_feature {
                        id,
                        feature,
                        enabled,
                    },
                    "Could not change the feature",
                )
                .await;
            daemon.refresh_features(id).await;
        }
        CrossDeviceCommand::SetHotspot { id, enabled } => {
            daemon
                .call(
                    &proto::set_hotspot { id, enabled },
                    "Could not switch the hotspot",
                )
                .await;
        }
        CrossDeviceCommand::SetUnicursorEdge { id, edge } => {
            daemon
                .call(
                    &proto::set_unicursor_edge { id, edge },
                    "Could not move the shared cursor edge",
                )
                .await;
        }
    }
}

/// The once-a-second clock the invitation countdown reads, running only
/// until the invitation expires or is cancelled.
#[derive(Default)]
struct Countdown {
    next_tick: Option<Instant>,
    expires_unix_ms: u64,
}

impl Countdown {
    /// Ticks at once, so the invitation never shows without a clock.
    fn start(&mut self, expires_unix_ms: u64, daemon: &Daemon) {
        self.expires_unix_ms = expires_unix_ms;
        self.tick(daemon);
    }

    fn stop(&mut self) {
        self.next_tick = None;
    }

    fn tick(&mut self, daemon: &Daemon) {
        let now_unix_ms = unix_millis();
        daemon.say(CrossDeviceEvent::Tick { now_unix_ms });
        self.next_tick = (now_unix_ms < self.expires_unix_ms).then(|| Instant::now() + TICK);
    }
}

/// Sleep until `deadline`, or forever when there is nothing pending.
async fn wait_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}
