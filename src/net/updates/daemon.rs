//! The connection to crownos-updated, and the discipline of getting it back.

use std::time::Duration;

use tokio::time::Instant;
use update_client::{ClientError, Event, UpdateClient};

use super::UpdatesEvent;
use crate::util::worker::MessageProxy;

/// The first retry delay; each failure doubles it up to [`LONGEST_RETRY`].
const FIRST_RETRY: Duration = Duration::from_secs(1);
const LONGEST_RETRY: Duration = Duration::from_secs(10);

const NOT_ALLOWED: &str = "Only an administrator can update this computer.";

/// Invariant: `client` being `None` always comes with a `retry_at`.
pub(super) struct Daemon {
    pub(super) client: Option<UpdateClient>,
    pub(super) retry_at: Option<Instant>,
    backoff: Duration,
    announced_offline: bool,
    proxy: MessageProxy<UpdatesEvent>,
}

impl Daemon {
    pub(super) fn new(proxy: MessageProxy<UpdatesEvent>) -> Self {
        Self {
            client: None,
            retry_at: Some(Instant::now()),
            backoff: FIRST_RETRY,
            announced_offline: false,
            proxy,
        }
    }

    pub(super) fn say(&self, event: UpdatesEvent) {
        let _ = self.proxy.message(event);
    }

    pub(super) async fn reconnect_when_due(&mut self) {
        if self.client.is_none() && self.retry_at.is_some_and(|due| due <= Instant::now()) {
            self.connect().await;
        }
    }

    async fn connect(&mut self) {
        self.retry_at = None;
        let connected = async {
            let mut client = UpdateClient::connect().await?;
            let snapshot = client.state().await?;
            Ok::<_, ClientError>((client, snapshot))
        };
        match connected.await {
            Ok((client, snapshot)) => {
                self.client = Some(client);
                self.backoff = FIRST_RETRY;
                self.announced_offline = false;
                self.say(UpdatesEvent::Online);
                self.say(UpdatesEvent::Snapshot(Box::new(snapshot)));
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
            self.say(UpdatesEvent::Offline);
        }
    }

    /// A connected client, reconnecting at once instead of at the backoff's tail.
    pub(super) async fn client(&mut self) -> Option<&mut UpdateClient> {
        if self.client.is_none() {
            self.connect().await;
        }
        self.client.as_mut()
    }

    /// Says why a verb failed. A lost connection only becomes the offline
    /// state; the offline card already tells the user.
    pub(super) fn report(&mut self, error: ClientError) {
        let message = match error {
            ClientError::Unavailable(_) => return self.lost(),
            ClientError::NotAllowed => NOT_ALLOWED.to_owned(),
            ClientError::WrongPhase(reason) => reason,
            ClientError::Ipc(source) => format!("The update service failed: {source}"),
        };
        self.say(UpdatesEvent::Error(message));
    }

    /// Hands queued events to the page and notices a closed socket.
    pub(super) fn forward_events(&mut self) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        for event in client.drain_events() {
            let _ = self.proxy.message(match event {
                Event::State(snapshot) => UpdatesEvent::Snapshot(snapshot),
                Event::Progress(progress) => UpdatesEvent::Progress {
                    phase: progress.phase,
                    fraction: progress.fraction,
                    line: progress.line,
                },
            });
        }
        if client.is_closed() {
            self.lost();
        }
    }
}
