//! The connection to crownconnect, and the discipline of getting it back.

use std::time::Duration;

use crownconnect_linux::ipc::proto::{self, DeviceId, DeviceInfo, Event};
use crownos_ipc::adapter::tokio::AsyncClient;
use crownos_ipc::{Client, Error as IpcError, EventMsg, MethodMsg};
use xilem::core::MessageProxy;
use xilem::tokio::time::Instant;

use super::CrossDeviceEvent;

/// The first retry delay; each failure doubles it up to [`LONGEST_RETRY`].
const FIRST_RETRY: Duration = Duration::from_secs(1);
const LONGEST_RETRY: Duration = Duration::from_secs(30);

pub(super) const NOT_RUNNING: &str = "CrownConnect is not running.";

/// Invariant: `client` being `None` always comes with a `retry_at`, so a
/// missing daemon is a timer the worker's `select!` is already waiting on.
pub(super) struct Daemon {
    pub(super) client: Option<AsyncClient>,
    pub(super) retry_at: Option<Instant>,
    backoff: Duration,
    announced_offline: bool,
    proxy: MessageProxy<CrossDeviceEvent>,
}

impl Daemon {
    pub(super) fn new(proxy: MessageProxy<CrossDeviceEvent>) -> Self {
        Self {
            client: None,
            retry_at: Some(Instant::now()),
            backoff: FIRST_RETRY,
            announced_offline: false,
            proxy,
        }
    }

    pub(super) fn say(&self, event: CrossDeviceEvent) {
        let _ = self.proxy.message(event);
    }

    pub(super) async fn reconnect_when_due(&mut self) {
        if self.client.is_none() && self.retry_at.is_some_and(|due| due <= Instant::now()) {
            self.connect().await;
        }
    }

    async fn connect(&mut self) {
        self.retry_at = None;
        match subscribed_client().await {
            Ok((client, devices)) => {
                self.client = Some(client);
                self.backoff = FIRST_RETRY;
                self.announced_offline = false;
                self.say(CrossDeviceEvent::Online);
                self.say(CrossDeviceEvent::Daemon(Event::DevicesChanged(
                    proto::DevicesChanged { devices },
                )));
            }
            Err(_) => self.lost(),
        }
    }

    /// Drop the client, arm the retry with backoff, say so once per outage.
    pub(super) fn lost(&mut self) {
        self.client = None;
        self.retry_at = Some(Instant::now() + self.backoff);
        self.backoff = (self.backoff * 2).min(LONGEST_RETRY);
        if !self.announced_offline {
            self.announced_offline = true;
            self.say(CrossDeviceEvent::Offline);
        }
    }

    /// One request, with every failure reported in words: a click while
    /// disconnected earns an immediate reconnect instead of the backoff's tail.
    pub(super) async fn call<M>(&mut self, request: &M, failure: &str) -> Option<M::Reply>
    where
        M: MethodMsg<Args = M>,
    {
        if self.client.is_none() {
            self.connect().await;
        }
        let Some(client) = self.client.as_mut() else {
            self.say(CrossDeviceEvent::Error(NOT_RUNNING.to_owned()));
            return None;
        };
        match client.call::<M>(request, Vec::new()).await {
            Ok(reply) => Some(reply),
            Err(IpcError::Remote(refusal)) => {
                self.say(CrossDeviceEvent::Error(format!(
                    "{failure}: {}",
                    refusal.message
                )));
                None
            }
            Err(_) => {
                self.lost();
                self.say(CrossDeviceEvent::Error(NOT_RUNNING.to_owned()));
                None
            }
        }
    }

    /// Fire-and-forget, and skipped while disconnected: a daemon that went
    /// away took whatever this would have closed with it.
    pub(super) async fn notify<M>(&mut self, message: &M)
    where
        M: MethodMsg<Args = M>,
    {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        if client.notify::<M>(message, Vec::new()).await.is_err() {
            self.lost();
        }
    }

    pub(super) async fn refresh_devices(&mut self) {
        if let Some(devices) = self
            .call(&proto::devices {}, "Could not list devices")
            .await
        {
            self.say(CrossDeviceEvent::Daemon(Event::DevicesChanged(
                proto::DevicesChanged { devices },
            )));
        }
    }

    pub(super) async fn refresh_features(&mut self, id: DeviceId) {
        if let Some(allowed) = self
            .call(
                &proto::features { id },
                "Could not read the device's features",
            )
            .await
        {
            self.say(CrossDeviceEvent::Features { id, allowed });
        }
    }

    /// Hand every queued push to the page, and notice a closed socket.
    pub(super) fn forward_events(&mut self) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        let inner = client.client_mut();
        forward(inner, &self.proxy, Event::DevicesChanged);
        forward(inner, &self.proxy, Event::DeviceConnected);
        forward(inner, &self.proxy, Event::FeatureChanged);
        forward(inner, &self.proxy, Event::FeatureFailed);
        forward(inner, &self.proxy, Event::BatteryChanged);
        forward(inner, &self.proxy, Event::HotspotChanged);
        forward(inner, &self.proxy, Event::PairingRequest);
        forward(inner, &self.proxy, Event::PairingResult);
        if inner.is_closed() {
            self.lost();
        }
    }
}

fn forward<E: EventMsg>(
    client: &mut Client,
    proxy: &MessageProxy<CrossDeviceEvent>,
    wrap: fn(E) -> Event,
) {
    while let Some(event) = client.next_event::<E>() {
        let _ = proxy.message(CrossDeviceEvent::Daemon(wrap(event)));
    }
}

/// A client that could miss a push is not yet a client, so connecting,
/// subscribing and the first listing are one step.
async fn subscribed_client() -> Result<(AsyncClient, Vec<DeviceInfo>), IpcError> {
    let mut client = AsyncClient::connect(proto::SERVICE)?;
    client.subscribe::<proto::DevicesChanged>().await?;
    client.subscribe::<proto::DeviceConnected>().await?;
    client.subscribe::<proto::FeatureChanged>().await?;
    client.subscribe::<proto::FeatureFailed>().await?;
    client.subscribe::<proto::BatteryChanged>().await?;
    client.subscribe::<proto::HotspotChanged>().await?;
    client.subscribe::<proto::PairingRequest>().await?;
    client.subscribe::<proto::PairingResult>().await?;
    let devices = client
        .call::<proto::devices>(&proto::devices {}, Vec::new())
        .await?;
    Ok((client, devices))
}
