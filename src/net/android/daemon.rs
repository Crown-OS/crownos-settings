//! The connection to crown-androidd: one subscribed client, re-made on the
//! next refresh or retry once it is lost.

use std::path::Path;
use std::time::Duration;

use android_protocol::{RuntimeState, StopReason, android};
use crownos_ipc::adapter::tokio::AsyncClient;
use crownos_ipc::{Error as IpcError, MethodMsg};
use tokio::time::Instant;

use super::survey::survey;
use super::{AndroidEvent, AndroidService};
use crate::util::worker::MessageProxy;

/// Where the package installs the daemon, which is what tells a daemon that is
/// not running from one that is not installed.
const DAEMON_EXECUTABLE: &str = "/usr/bin/crown-androidd";

/// How long an unreachable daemon is left before it is asked again.
const RETRY: Duration = Duration::from_secs(10);

const NOT_RUNNING: &str = "The Android service isn't running.";

/// Invariant: `retry_at` is set only while Android apps are set up and the
/// daemon cannot be reached, so the worker waits on a timer only then.
pub(super) struct Daemon {
    pub(super) client: Option<AsyncClient>,
    pub(super) retry_at: Option<Instant>,
    /// From the last survey: whether to ask for the registration id.
    google_play: bool,
    installed: bool,
    /// The id never changes once Android has one, so it is asked for until then.
    registration_id_known: bool,
    proxy: MessageProxy<AndroidEvent>,
}

impl Daemon {
    pub(super) fn new(proxy: MessageProxy<AndroidEvent>) -> Self {
        Self {
            client: None,
            retry_at: None,
            google_play: false,
            installed: false,
            registration_id_known: false,
            proxy,
        }
    }

    fn say(&self, event: AndroidEvent) {
        let _ = self.proxy.message(event);
    }

    /// Everything again: the disk, then the daemon.
    pub(super) async fn refresh(&mut self) {
        self.reread_disk().await;
        self.ask_status().await;
    }

    /// The daemon's status alone, which is all a retry needs.
    pub(super) async fn ask_status(&mut self) {
        self.retry_at = None;
        match self.request(&android::status {}).await {
            Ok(status) => {
                let running = status.state == RuntimeState::Running;
                self.say(AndroidEvent::Service(AndroidService::Answered(status)));
                if running {
                    self.ask_registration_id().await;
                }
            }
            Err(_) => self.unreachable(),
        }
    }

    async fn reread_disk(&mut self) {
        let survey = survey().await;
        let config = survey.installed.as_ref();
        self.installed = config.is_some_and(|config| config.enabled);
        self.google_play = config.is_some_and(|config| config.enabled && config.google_play);
        self.registration_id_known &= self.installed;
        self.say(AndroidEvent::Installed(survey.installed));
        self.say(AndroidEvent::Setup(survey.setup));
        self.say(AndroidEvent::Storage(survey.storage));
    }

    pub(super) async fn stop(&mut self) {
        let request = android::stop {
            reason: StopReason::User,
        };
        if let Err(message) = self.request(&request).await {
            self.say(AndroidEvent::Failed(format!(
                "Android could not be stopped: {message}"
            )));
        }
    }

    async fn ask_registration_id(&mut self) {
        if !self.google_play || self.registration_id_known {
            return;
        }
        let id = self
            .request(&android::google_registration_id {})
            .await
            .ok()
            .flatten();
        self.registration_id_known = id.is_some();
        self.say(AndroidEvent::RegistrationId(id));
    }

    /// One request, connecting first when there is no client. `Err` is the
    /// daemon's refusal in its own words, or that it is not running.
    async fn request<M>(&mut self, request: &M) -> Result<M::Reply, String>
    where
        M: MethodMsg<Args = M>,
    {
        if self.client.is_none() {
            self.client = subscribed_client().await.ok();
        }
        let client = self.client.as_mut().ok_or(NOT_RUNNING)?;
        match client.call::<M>(request, Vec::new()).await {
            Ok(reply) => Ok(reply),
            Err(IpcError::Remote(refusal)) => Err(refusal.message),
            Err(_) => {
                self.client = None;
                Err(NOT_RUNNING.to_owned())
            }
        }
    }

    /// Say why the daemon did not answer, and come back later if it should
    /// have.
    pub(super) fn unreachable(&mut self) {
        self.client = None;
        let installed = self.installed && Path::new(DAEMON_EXECUTABLE).exists();
        let service = if installed {
            self.retry_at = Some(Instant::now() + RETRY);
            AndroidService::NotRunning
        } else {
            AndroidService::NotInstalled
        };
        self.say(AndroidEvent::Service(service));
    }

    /// Hand the page what the daemon pushed: the newest runtime status and
    /// setup record. A setup that just finished changed `android.ron`, so the
    /// disk is read again.
    pub(super) async fn forward_events(&mut self) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        let inner = client.client_mut();
        let runtime = std::iter::from_fn(|| inner.next_event::<android::RuntimeChanged>()).last();
        let setup = std::iter::from_fn(|| inner.next_event::<android::SetupProgress>()).last();
        let closed = inner.is_closed();

        if let Some(progress) = setup {
            let finished = progress.status.stage.is_final();
            self.say(AndroidEvent::Setup(Some(progress.status)));
            if finished {
                self.reread_disk().await;
            }
        }
        if let Some(changed) = runtime {
            let running = changed.status.state == RuntimeState::Running;
            self.say(AndroidEvent::Service(AndroidService::Answered(
                changed.status,
            )));
            if running {
                self.ask_registration_id().await;
            }
        }
        if closed {
            self.unreachable();
        }
    }
}

/// A client that could miss a change is not yet a client.
async fn subscribed_client() -> Result<AsyncClient, IpcError> {
    let mut client = AsyncClient::connect(android::SERVICE)?;
    client.subscribe::<android::RuntimeChanged>().await?;
    client.subscribe::<android::SetupProgress>().await?;
    Ok(client)
}
