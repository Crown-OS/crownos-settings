//! The compositor's control socket, for the session restart that some
//! releases ask for.
//!
//! The compositor can exit before its reply arrives. Thus a closed socket or
//! no reply in [`REPLY_WAIT`] counts as a restart that started.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use crownos_ipc::Error as IpcError;
use crownpositor_control::crownpositor;

const RESTART_SESSION: &str = "restart-session";
const REPLY_WAIT: Duration = Duration::from_secs(2);

pub async fn restart_session() -> Result<()> {
    tokio::task::spawn_blocking(|| {
        let compositor =
            crownpositor::Client::connect().context("Could not reach the compositor")?;
        request_session_restart(compositor)
    })
    .await
    .context("The session restart did not run")?
}

fn request_session_restart(mut compositor: crownpositor::Client) -> Result<()> {
    let pending = compositor
        .action(RESTART_SESSION.to_owned())
        .context("Could not ask the compositor to restart")?;
    match compositor.wait(pending, Some(REPLY_WAIT)) {
        Ok(()) | Err(IpcError::Disconnected | IpcError::Timeout) => Ok(()),
        Err(IpcError::Remote(refusal)) => bail!("{}", refusal.message),
        Err(other) => Err(other).context("The compositor did not answer"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::os::unix::net::UnixListener;
    use std::path::{Path, PathBuf};

    use crownos_ipc::{RemoteError, ServiceBuilder, blocking};

    use super::*;

    struct Locked;

    impl crownpositor::Handler for Locked {
        fn action(&mut self, _spelling: String) -> Result<(), RemoteError> {
            Err(RemoteError::new(
                RemoteError::UNAUTHORIZED,
                "the session is locked",
            ))
        }
    }

    fn socket_directory(case: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "crownsettings-session-{case}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        directory
    }

    fn connect(directory: &Path) -> Result<crownpositor::Client> {
        Ok(crownpositor::Client::connect_in(directory)?)
    }

    #[test]
    fn a_compositor_that_exits_before_it_replies_has_restarted() -> Result<()> {
        let directory = socket_directory("exits");
        std::fs::create_dir_all(&directory)?;
        let listener =
            UnixListener::bind(directory.join(format!("{}.sock", crownpositor::SERVICE)))?;
        let compositor = std::thread::spawn(move || -> std::io::Result<()> {
            let (mut stream, _) = listener.accept()?;
            let mut request = [0; 64];
            let _ = stream.read(&mut request)?;
            Ok(())
        });

        request_session_restart(connect(&directory)?)?;
        let _ = compositor.join();
        std::fs::remove_dir_all(&directory)?;
        Ok(())
    }

    #[test]
    fn a_compositor_that_stops_before_it_replies_has_restarted() -> Result<()> {
        let directory = socket_directory("silent");
        std::fs::create_dir_all(&directory)?;
        let listener =
            UnixListener::bind(directory.join(format!("{}.sock", crownpositor::SERVICE)))?;
        let (stopped, wait_for_test) = std::sync::mpsc::channel::<()>();
        let compositor = std::thread::spawn(move || -> std::io::Result<()> {
            let (_stream, _) = listener.accept()?;
            let _ = wait_for_test.recv();
            Ok(())
        });

        request_session_restart(connect(&directory)?)?;
        drop(stopped);
        let _ = compositor.join();
        std::fs::remove_dir_all(&directory)?;
        Ok(())
    }

    #[test]
    fn a_locked_session_refuses_the_restart() -> Result<()> {
        let directory = socket_directory("locked");
        let mut server = ServiceBuilder::new(crownpositor::SERVICE)
            .directory(&directory)
            .build()?;
        std::thread::spawn(move || {
            blocking::serve(&mut server, |server, peer, message| {
                crownpositor::dispatch(&mut Locked, server, peer, message).map(|()| true)
            })
        });

        let refusal = request_session_restart(connect(&directory)?)
            .err()
            .map(|error| error.to_string());
        assert_eq!(refusal.as_deref(), Some("the session is locked"));
        std::fs::remove_dir_all(&directory)?;
        Ok(())
    }
}
