//! crown-voiced, the voice enhancement service: which backend and device it
//! enhances on, for the Voice page's status row.
//!
//! The house shape (see [`crate::net`]), cut down to what a status row needs:
//! one [`VoiceCommand`], answered by one `status()` call whose outcome is the
//! event. There is no subscription and no reconnect timer. The daemon is
//! socket-activated, so asking is what starts it, and the page asks only when
//! it opens and when enhancement for all apps is switched on — a page that
//! kept polling would keep a daemon nobody uses awake.

mod status;

use crownui::prelude::{Cx, Signal};
use tokio::sync::mpsc::UnboundedSender;

use crate::util::worker::spawn_worker;

/// Everything the Voice page can ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceCommand {
    Refresh,
}

/// What the last `status()` call found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum VoiceService {
    /// No answer yet.
    #[default]
    Checking,
    /// The daemon answered. `backend` is the daemon's own "name: device"
    /// label, or `None` when no backend can enhance on this machine.
    Running {
        backend: Option<String>,
    },
    /// The daemon is installed but nothing answers its socket.
    NotRunning,
    NotInstalled,
}

/// The voice half of the app's [`Store`](crate::state::Store).
#[derive(Debug, Default)]
pub struct VoiceState {
    sender: Option<UnboundedSender<VoiceCommand>>,
    pub service: VoiceService,
}

impl VoiceState {
    /// Ask the daemon again. Silent without a worker, as in tests.
    pub fn refresh(&self) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(VoiceCommand::Refresh);
        }
    }
}

/// Starts the voice backend for as long as the calling component lives.
pub fn voice_worker(cx: &mut Cx, voice: Signal<VoiceState>) {
    let sender = spawn_worker(cx, status::run, move |cx, service| {
        voice.update(cx, |state| state.service = service);
    });
    voice.update(cx, |state| state.sender = Some(sender));
}
