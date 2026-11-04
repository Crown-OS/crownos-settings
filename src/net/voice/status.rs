//! One `status()` call, and what its answer means for the status row.

use std::cmp::Reverse;
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
            backend: serving_backend(&status.backends),
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

/// The backend a stream runs on: one already loaded, else the one the daemon
/// would load first — the highest real-time score, ties to the earlier entry.
fn serving_backend(backends: &[BackendInfo]) -> Option<String> {
    backends
        .iter()
        .filter(|backend| backend.realtime_score > 0)
        .min_by_key(|backend| (Reverse(backend.loaded), Reverse(backend.realtime_score)))
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

    fn backend(name: &str, device: &str, realtime_score: u32, loaded: bool) -> BackendInfo {
        BackendInfo {
            name: name.to_owned(),
            device: device.to_owned(),
            realtime_score,
            offline_score: realtime_score,
            loaded,
        }
    }

    #[test]
    fn the_loaded_backend_is_the_one_in_use() {
        let backends = [
            backend("onnx", "Radeon 780M", 90, false),
            backend("cpu", "Ryzen 7 7840U", 40, true),
        ];
        assert_eq!(
            serving_backend(&backends).as_deref(),
            Some("cpu: Ryzen 7 7840U")
        );
    }

    #[test]
    fn an_idle_daemon_names_the_backend_it_would_load_first() {
        let backends = [
            backend("passthrough", "", 1, false),
            backend("cpu", "Ryzen 7 7840U", 40, false),
            backend("onnx", "Radeon 780M", 40, false),
        ];
        assert_eq!(
            serving_backend(&backends).as_deref(),
            Some("cpu: Ryzen 7 7840U"),
            "a tie goes to the earlier entry"
        );
        assert_eq!(
            serving_backend(&[backend("passthrough", "", 1, false)]).as_deref(),
            Some("passthrough"),
            "a backend without a device is named alone"
        );
    }

    #[test]
    fn a_backend_that_cannot_run_live_is_never_named() {
        assert_eq!(
            serving_backend(&[backend("onnx", "Radeon 780M", 0, true)]),
            None
        );
        assert_eq!(serving_backend(&[]), None);
    }
}
