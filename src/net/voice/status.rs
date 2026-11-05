//! One `status()` call, and what its answer means for the status row.

use std::path::Path;
use std::time::Duration;

use crownos_ipc::Error as IpcError;
use crownos_ipc::adapter::tokio::AsyncClient;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::timeout;
use voice_protocol::{BackendInfo, ServiceStatus, voice};

use super::{VoiceCommand, VoiceService};
use crate::util::worker::MessageProxy;

/// Where the package installs the daemon, which is what tells a daemon that is
/// not running from one that is not installed.
const DAEMON_EXECUTABLE: &str = "/usr/bin/crown-voiced";

/// Generous, because the first call after login starts the daemon, which
/// probes every backend before it answers.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);

/// Returns only when the command channel closes, i.e. with the window.
pub(super) async fn run(
    proxy: MessageProxy<VoiceService>,
    mut commands: UnboundedReceiver<VoiceCommand>,
) {
    while let Some(VoiceCommand::Refresh) = commands.recv().await {
        if proxy.message(query().await).is_err() {
            return;
        }
    }
}

async fn query() -> VoiceService {
    match timeout(ANSWER_TIMEOUT, ask_status()).await {
        Ok(Ok(status)) => VoiceService::Running {
            backend: serving_backend(&status),
        },
        Ok(Err(IpcError::ServiceUnavailable { .. })) if !Path::new(DAEMON_EXECUTABLE).exists() => {
            VoiceService::NotInstalled
        }
        _ => VoiceService::NotRunning,
    }
}

async fn ask_status() -> Result<ServiceStatus, IpcError> {
    let mut client = AsyncClient::connect(voice::SERVICE)?;
    client
        .call::<voice::status>(&voice::status {}, Vec::new())
        .await
}

/// The backend the daemon itself says a new live stream would run on.
fn serving_backend(status: &ServiceStatus) -> Option<String> {
    let name = status.realtime_backend.as_deref()?;
    status
        .backends
        .iter()
        .find(|backend| backend.name == name)
        .map(label)
}

/// The daemon's own wording, as `EnhancedNode::backend` spells it.
fn label(backend: &BackendInfo) -> String {
    if backend.device.is_empty() {
        backend.name.clone()
    } else {
        format!("{}: {}", backend.name, backend.device)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend(name: &str, device: &str) -> BackendInfo {
        BackendInfo {
            name: name.to_owned(),
            device: device.to_owned(),
            realtime_score: 40,
            offline_score: 40,
            loaded: false,
        }
    }

    fn status(realtime_backend: Option<&str>) -> ServiceStatus {
        ServiceStatus {
            backends: vec![
                backend("onnx", "Radeon 780M"),
                backend("cpu", "Ryzen 7 7840U"),
                backend("passthrough", ""),
            ],
            streams: 0,
            jobs: 0,
            realtime_backend: realtime_backend.map(str::to_owned),
            offline_backend: None,
        }
    }

    #[test]
    fn names_the_backend_the_daemon_selected() {
        assert_eq!(
            serving_backend(&status(Some("cpu"))).as_deref(),
            Some("cpu: Ryzen 7 7840U")
        );
        assert_eq!(
            serving_backend(&status(Some("passthrough"))).as_deref(),
            Some("passthrough"),
            "a backend without a device is named alone"
        );
    }

    #[test]
    fn no_selection_names_nothing() {
        assert_eq!(serving_backend(&status(None)), None);
        assert_eq!(serving_backend(&status(Some("maxine"))), None);
    }
}
