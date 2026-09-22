//! The Wayland half: one `zwlr_output_management_v1` conversation.
//!
//! No Wayland type escapes this file. What does is a [`Session`]: connect it,
//! then alternately await what the compositor has to say and hand it a
//! configuration.
//!
//! ## Why this is not a thread
//!
//! `wayland-client`'s own dispatch blocks, and a thread parked in
//! `blocking_dispatch` cannot notice a command arriving on a channel — it
//! wakes only when the *compositor* speaks, which on a desk where nothing is
//! being plugged in is never. Everything the page asked for would sit in the
//! channel until an unrelated hotplug flushed it out.
//!
//! So readiness comes from the connection's file descriptor instead, and the
//! whole conversation rides xilem's tokio runtime like the other two backends
//! in [`crate::net`]. Commands are then just another branch of the same
//! `select!`, and there is no thread to synchronise with.

use std::io::ErrorKind;
use std::os::fd::{AsFd, AsRawFd, RawFd};

use wayland_client::{
    Connection, Dispatch, DispatchError, EventQueue, Proxy, QueueHandle, WEnum,
    backend::WaylandError,
    event_created_child,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_output, wl_registry},
};
use wayland_protocols_wlr::output_management::v1::client::{
    zwlr_output_configuration_head_v1::ZwlrOutputConfigurationHeadV1,
    zwlr_output_configuration_v1::{self, ZwlrOutputConfigurationV1},
    zwlr_output_head_v1::{self, ZwlrOutputHeadV1},
    zwlr_output_manager_v1::{self, ZwlrOutputManagerV1},
    zwlr_output_mode_v1::{self, ZwlrOutputModeV1},
};
use xilem::tokio::io::unix::AsyncFd;

use super::{Mode, Monitor, MonitorDraft, Outcome, OutputEvent, Transform};

/// The newest manager version this client can speak. 4 adds adaptive sync.
const VERSION: std::ops::RangeInclusive<u32> = 1..=4;

/// Everything a configuration needs that only a version 4 manager has.
const ADAPTIVE_SYNC_SINCE: u32 = 4;

/// One live conversation with the compositor's output manager.
pub struct Session {
    connection: Connection,
    /// The connection's descriptor, registered with the runtime's poller.
    /// Borrowed, not owned — `connection` is what closes it.
    readiness: AsyncFd<RawFd>,
    queue: EventQueue<State>,
    manager: ZwlrOutputManagerV1,
    state: State,
}

impl Session {
    /// Opens the conversation, or says in one human sentence why there is none.
    ///
    /// A separate connection from the one winit holds for the window: this is
    /// a session-wide conversation about every monitor rather than anything to
    /// do with this app's surface, and winit does not expose its own.
    pub fn connect() -> Result<Self, String> {
        let connection = Connection::connect_to_env()
            .map_err(|err| format!("no Wayland connection: {err}"))?;

        let (globals, queue) = registry_queue_init::<State>(&connection)
            .map_err(|err| format!("the Wayland registry is unreadable: {err}"))?;

        let manager: ZwlrOutputManagerV1 = globals
            .bind(&queue.handle(), VERSION, ())
            .map_err(|_| "this compositor does not support output management".to_owned())?;

        let readiness = AsyncFd::new(connection.as_fd().as_raw_fd())
            .map_err(|err| format!("the Wayland connection cannot be polled: {err}"))?;

        Ok(Self {
            connection,
            readiness,
            queue,
            manager,
            state: State::default(),
        })
    }

    /// Waits for the compositor to say something, and reports a new snapshot
    /// if that is what it said.
    ///
    /// Cancel-safe: the only await is on the descriptor becoming readable, so
    /// losing the race in a `select!` drops a read that had not happened.
    pub async fn dispatch(&mut self, report: &impl Fn(OutputEvent)) -> Result<(), DispatchError> {
        self.pump().await?;
        self.report_snapshot(report);
        Ok(())
    }

    /// Builds and submits one configuration, then waits for its verdict.
    ///
    /// Every head must be named — the protocol makes omitting one an error —
    /// so heads the page has no draft for are re-sent exactly as they are.
    pub async fn configure(
        &mut self,
        drafts: &[MonitorDraft],
        test_only: bool,
        report: &impl Fn(OutputEvent),
    ) -> Result<(), DispatchError> {
        let Some(serial) = self.state.serial else {
            report(OutputEvent::Result {
                outcome: Outcome::Failed("the compositor has not described its outputs yet".into()),
                test: test_only,
            });
            return Ok(());
        };

        let qh = self.queue.handle();
        let configuration = self.manager.create_configuration(serial, &qh, ());
        let adaptive_sync = self.manager.version() >= ADAPTIVE_SYNC_SINCE;

        for (handle, head) in &self.state.heads {
            // A head with no draft keeps what it has, being off included:
            // enabling one unasked is how a monitor the user deliberately
            // switched off would come back on by itself.
            let draft = drafts.iter().find(|draft| draft.name == head.name);
            if !draft.map_or(head.enabled, |draft| draft.enabled) {
                configuration.disable_head(handle);
                continue;
            }

            let configured = configuration.enable_head(handle, &qh, ());
            let Some(draft) = draft else {
                continue;
            };

            configured.set_position(draft.position.0, draft.position.1);
            configured.set_scale(draft.scale);
            configured.set_transform(transform_to_wire(draft.transform));

            if let Some(index) = draft.mode
                && let Some((mode, _)) = head.modes.get(index)
            {
                configured.set_mode(mode);
            }

            if adaptive_sync && head.adaptive_sync.is_some() {
                configured.set_adaptive_sync(if draft.adaptive_sync {
                    zwlr_output_head_v1::AdaptiveSyncState::Enabled
                } else {
                    zwlr_output_head_v1::AdaptiveSyncState::Disabled
                });
            }
        }

        if test_only {
            configuration.test();
        } else {
            configuration.apply();
        }

        self.state.outcome = None;
        while self.state.outcome.is_none() {
            self.pump().await?;
            self.report_snapshot(report);
        }
        configuration.destroy();

        if let Some(outcome) = self.state.outcome.take() {
            report(OutputEvent::Result {
                outcome,
                test: test_only,
            });
        }
        Ok(())
    }

    fn report_snapshot(&mut self, report: &impl Fn(OutputEvent)) {
        if self.state.take_done() {
            report(OutputEvent::Snapshot(self.state.monitors()));
        }
    }

    /// Dispatches at least one event, waiting for the socket if need be.
    ///
    /// The read guard is taken *after* the await rather than around it. It is
    /// not `Send`, and holding one across a suspension point would make this
    /// whole future `!Send` — which xilem's worker will not accept. Nothing is
    /// lost by the later claim: anything that arrived in the meantime makes
    /// `prepare_read` decline, and the next turn dispatches it.
    async fn pump(&mut self) -> Result<(), DispatchError> {
        loop {
            if self.queue.dispatch_pending(&mut self.state)? > 0 {
                return Ok(());
            }
            self.connection.flush().map_err(DispatchError::Backend)?;

            let mut ready = self
                .readiness
                .readable()
                .await
                .map_err(|err| DispatchError::Backend(WaylandError::Io(err)))?;

            let Some(guard) = self.queue.prepare_read() else {
                // Events are already queued; the next turn dispatches them.
                continue;
            };

            // Clearing the readiness on an empty read is what makes this
            // loop a loop rather than a spin. `readable()` reports a *flag*,
            // not the socket, and the flag stays raised until something
            // clears it — so a read that turns up nothing (a stale flag, or
            // another queue got there first) would otherwise be followed by a
            // `readable()` that returns at once, forever. The task then never
            // parks, the runtime's reactor never re-polls the descriptor, and
            // the reply being waited for is never read at all.
            match guard.read() {
                Ok(0) => ready.clear_ready(),
                Ok(_) => {}
                Err(WaylandError::Io(err)) if err.kind() == ErrorKind::WouldBlock => {
                    ready.clear_ready();
                }
                Err(err) => return Err(DispatchError::Backend(err)),
            }
        }
    }
}

// --- MARK: Accumulated state ---

#[derive(Default)]
struct State {
    heads: Vec<(ZwlrOutputHeadV1, Head)>,
    serial: Option<u32>,
    done: bool,
    outcome: Option<Outcome>,
}

impl State {
    /// Whether a fresh `done` has arrived since this was last asked.
    fn take_done(&mut self) -> bool {
        std::mem::take(&mut self.done)
    }

    fn head_mut(&mut self, handle: &ZwlrOutputHeadV1) -> Option<&mut Head> {
        self.heads
            .iter_mut()
            .find(|(existing, _)| existing == handle)
            .map(|(_, head)| head)
    }

    /// The snapshot the UI sees, with no Wayland types in it.
    fn monitors(&self) -> Vec<Monitor> {
        self.heads
            .iter()
            .map(|(_, head)| Monitor {
                name: head.name.clone(),
                description: head.description.clone(),
                make: head.make.clone(),
                model: head.model.clone(),
                serial: head.serial.clone(),
                enabled: head.enabled,
                modes: head.modes.iter().map(|(_, mode)| mode.clone()).collect(),
                current_mode: head.current_mode.as_ref().and_then(|current| {
                    head.modes.iter().position(|(handle, _)| handle == current)
                }),
                position: head.position,
                scale: head.scale,
                transform: head.transform,
                adaptive_sync: head.adaptive_sync.unwrap_or(false),
                vrr_capable: head.adaptive_sync.is_some(),
            })
            .collect()
    }
}

#[derive(Default)]
struct Head {
    name: String,
    description: String,
    make: Option<String>,
    model: Option<String>,
    serial: Option<String>,
    modes: Vec<(ZwlrOutputModeV1, Mode)>,
    current_mode: Option<ZwlrOutputModeV1>,
    enabled: bool,
    position: (i32, i32),
    scale: f64,
    transform: Transform,
    /// `None` until the head says so, which a compositor only does for a head
    /// whose connector can actually do it — the protocol has no capability
    /// event, so the presence of this one is the whole signal, and it is what
    /// decides whether the page offers the toggle at all.
    adaptive_sync: Option<bool>,
}

// --- MARK: Dispatch ---

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrOutputManagerV1, ()> for State {
    fn event(
        state: &mut Self,
        _proxy: &ZwlrOutputManagerV1,
        event: zwlr_output_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_output_manager_v1::Event::Head { head } => {
                state.heads.push((head, Head::default()));
            }
            zwlr_output_manager_v1::Event::Done { serial } => {
                state.serial = Some(serial);
                state.done = true;
            }
            _ => {}
        }
    }

    event_created_child!(State, ZwlrOutputManagerV1, [
        zwlr_output_manager_v1::EVT_HEAD_OPCODE => (ZwlrOutputHeadV1, ()),
    ]);
}

impl Dispatch<ZwlrOutputHeadV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &ZwlrOutputHeadV1,
        event: zwlr_output_head_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let zwlr_output_head_v1::Event::Finished = event {
            state.heads.retain(|(existing, _)| existing != proxy);
            return;
        }

        let Some(head) = state.head_mut(proxy) else {
            return;
        };
        match event {
            zwlr_output_head_v1::Event::Name { name } => head.name = name,
            zwlr_output_head_v1::Event::Description { description } => {
                head.description = description;
            }
            zwlr_output_head_v1::Event::Make { make } => head.make = Some(make),
            zwlr_output_head_v1::Event::Model { model } => head.model = Some(model),
            zwlr_output_head_v1::Event::SerialNumber { serial_number } => {
                head.serial = Some(serial_number);
            }
            zwlr_output_head_v1::Event::Mode { mode } => {
                head.modes.push((mode, Mode::default()));
            }
            zwlr_output_head_v1::Event::CurrentMode { mode } => head.current_mode = Some(mode),
            zwlr_output_head_v1::Event::Enabled { enabled } => head.enabled = enabled != 0,
            zwlr_output_head_v1::Event::Position { x, y } => head.position = (x, y),
            zwlr_output_head_v1::Event::Scale { scale } => head.scale = scale,
            zwlr_output_head_v1::Event::Transform { transform } => {
                head.transform = transform_from_wire(transform);
            }
            zwlr_output_head_v1::Event::AdaptiveSync { state: sync } => {
                head.adaptive_sync = Some(matches!(
                    sync,
                    WEnum::Value(zwlr_output_head_v1::AdaptiveSyncState::Enabled)
                ));
            }
            _ => {}
        }
    }

    event_created_child!(State, ZwlrOutputHeadV1, [
        zwlr_output_head_v1::EVT_MODE_OPCODE => (ZwlrOutputModeV1, ()),
    ]);
}

impl Dispatch<ZwlrOutputModeV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &ZwlrOutputModeV1,
        event: zwlr_output_mode_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some((_, mode)) = state
            .heads
            .iter_mut()
            .flat_map(|(_, head)| head.modes.iter_mut())
            .find(|(handle, _)| handle == proxy)
        else {
            return;
        };

        match event {
            zwlr_output_mode_v1::Event::Size { width, height } => {
                mode.width = width;
                mode.height = height;
            }
            zwlr_output_mode_v1::Event::Refresh { refresh } => mode.refresh = refresh,
            zwlr_output_mode_v1::Event::Preferred => mode.preferred = true,
            _ => {}
        }
    }
}

impl Dispatch<ZwlrOutputConfigurationV1, ()> for State {
    fn event(
        state: &mut Self,
        _proxy: &ZwlrOutputConfigurationV1,
        event: zwlr_output_configuration_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        state.outcome = Some(match event {
            zwlr_output_configuration_v1::Event::Succeeded => Outcome::Succeeded,
            zwlr_output_configuration_v1::Event::Failed => {
                Outcome::Failed("the compositor could not apply that arrangement".into())
            }
            // The monitors changed underneath the configuration — a hotplug
            // between building it and sending it — which is a retry, not
            // something the user did wrong.
            zwlr_output_configuration_v1::Event::Cancelled => Outcome::Cancelled,
            _ => return,
        });
    }
}

impl Dispatch<ZwlrOutputConfigurationHeadV1, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrOutputConfigurationHeadV1,
        _event: <ZwlrOutputConfigurationHeadV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

// --- MARK: Wire conversions ---

fn transform_from_wire(transform: WEnum<wl_output::Transform>) -> Transform {
    use wl_output::Transform as Wl;
    match transform.into_result() {
        Ok(Wl::_90) => Transform::Rotate90,
        Ok(Wl::_180) => Transform::Rotate180,
        Ok(Wl::_270) => Transform::Rotate270,
        Ok(Wl::Flipped) => Transform::Flipped,
        Ok(Wl::Flipped90) => Transform::Flipped90,
        Ok(Wl::Flipped180) => Transform::Flipped180,
        Ok(Wl::Flipped270) => Transform::Flipped270,
        _ => Transform::Normal,
    }
}

fn transform_to_wire(transform: Transform) -> wl_output::Transform {
    use wl_output::Transform as Wl;
    match transform {
        Transform::Normal => Wl::Normal,
        Transform::Rotate90 => Wl::_90,
        Transform::Rotate180 => Wl::_180,
        Transform::Rotate270 => Wl::_270,
        Transform::Flipped => Wl::Flipped,
        Transform::Flipped90 => Wl::Flipped90,
        Transform::Flipped180 => Wl::Flipped180,
        Transform::Flipped270 => Wl::Flipped270,
    }
}
