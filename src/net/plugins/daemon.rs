//! The connection to crownplugind, and the discipline of getting it back.

use std::time::Duration;

use crownos_ipc::adapter::tokio::AsyncClient;
use crownos_ipc::{Error as IpcError, MethodMsg};
use crownplugin_proto::plugind;
use tokio::time::Instant;

use super::{EntityInfo, Kind, PluginsEvent};
use crate::util::worker::MessageProxy;

/// The first retry delay; each failure doubles it up to [`LONGEST_RETRY`].
const FIRST_RETRY: Duration = Duration::from_secs(1);
/// Short enough that a restarted daemon is back before the user wonders.
const LONGEST_RETRY: Duration = Duration::from_secs(10);

pub(super) const NOT_RUNNING: &str = "Plugins aren't running.";

/// Why a request produced no reply.
pub(super) enum Failure {
    /// The daemon answered with a refusal, in its own words.
    Refused(String),
    Offline,
}

/// Invariant: `client` being `None` always comes with a `retry_at`, so a
/// missing daemon is a timer the worker's `select!` is already waiting on.
pub(super) struct Daemon {
    pub(super) client: Option<AsyncClient>,
    pub(super) retry_at: Option<Instant>,
    backoff: Duration,
    announced_offline: bool,
    /// The list has not been read since it last changed or since connecting.
    stale: bool,
    proxy: MessageProxy<PluginsEvent>,
}

impl Daemon {
    pub(super) fn new(proxy: MessageProxy<PluginsEvent>) -> Self {
        Self {
            client: None,
            retry_at: Some(Instant::now()),
            backoff: FIRST_RETRY,
            announced_offline: false,
            stale: false,
            proxy,
        }
    }

    pub(super) fn say(&self, event: PluginsEvent) {
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
            Ok(client) => {
                self.client = Some(client);
                self.backoff = FIRST_RETRY;
                self.announced_offline = false;
                self.stale = true;
                self.say(PluginsEvent::Online);
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
            self.say(PluginsEvent::Offline);
        }
    }

    /// One request. A request while disconnected earns an immediate
    /// reconnect instead of the backoff's tail.
    pub(super) async fn request<M>(&mut self, request: &M) -> Result<M::Reply, Failure>
    where
        M: MethodMsg<Args = M>,
    {
        if self.client.is_none() {
            self.connect().await;
        }
        let client = self.client.as_mut().ok_or(Failure::Offline)?;
        match client.call::<M>(request, Vec::new()).await {
            Ok(reply) => Ok(reply),
            Err(IpcError::Remote(refusal)) => Err(Failure::Refused(refusal.message)),
            Err(_) => {
                self.lost();
                Err(Failure::Offline)
            }
        }
    }

    /// [`request`](Self::request), with any failure reported in words.
    pub(super) async fn call<M>(&mut self, request: &M, failure: &str) -> Option<M::Reply>
    where
        M: MethodMsg<Args = M>,
    {
        match self.request(request).await {
            Ok(reply) => Some(reply),
            Err(Failure::Refused(message)) => {
                self.say(PluginsEvent::Error(format!("{failure}: {message}")));
                None
            }
            Err(Failure::Offline) => {
                self.say(PluginsEvent::Error(NOT_RUNNING.to_owned()));
                None
            }
        }
    }

    /// The list, then the values of every plugin that declares settings.
    pub(super) async fn refresh(&mut self) {
        let Some(entities) = self.call(&plugind::list {}, "Could not list plugins").await else {
            return;
        };
        let with_settings: Vec<String> = entities
            .iter()
            .filter(|entity| declares_settings(entity))
            .map(|entity| entity.id.clone())
            .collect();
        self.say(PluginsEvent::Listed(entities));
        for id in with_settings {
            self.load_settings(id).await;
        }
    }

    pub(super) async fn load_settings(&mut self, id: String) {
        if let Some(values) = self
            .call(
                &plugind::settings { id: id.clone() },
                "Could not read the plugin's settings",
            )
            .await
        {
            self.say(PluginsEvent::Values { id, values });
        }
    }

    pub(super) async fn load_logs(&mut self, id: String) {
        if let Some(lines) = self
            .call(&plugind::logs { id: id.clone() }, "Could not read the logs")
            .await
        {
            self.say(PluginsEvent::Logs { id, lines });
        }
    }

    /// Hand queued log lines to the page, notice a closed socket, and say
    /// whether the list has to be read again.
    #[must_use]
    pub(super) fn forward_events(&mut self) -> bool {
        let Some(client) = self.client.as_mut() else {
            return false;
        };
        let inner = client.client_mut();
        let mut stale = std::mem::take(&mut self.stale);
        while inner.next_event::<plugind::EntitiesChanged>().is_some() {
            stale = true;
        }
        while let Some(logged) = inner.next_event::<plugind::Logged>() {
            let _ = self.proxy.message(PluginsEvent::Logged(logged.line));
        }
        if inner.is_closed() {
            self.lost();
            return false;
        }
        stale
    }
}

fn declares_settings(entity: &EntityInfo) -> bool {
    entity.kind == Kind::Plugin && entity.settings.is_some()
}

/// A client that could miss a change is not yet a client.
async fn subscribed_client() -> Result<AsyncClient, IpcError> {
    let mut client = AsyncClient::connect(plugind::SERVICE)?;
    client.subscribe::<plugind::EntitiesChanged>().await?;
    client.subscribe::<plugind::Logged>().await?;
    Ok(client)
}
