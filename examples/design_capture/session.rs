//! One headless Settings window over a store the capture fills in.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use anyhow::Result;
use capture::CaptureApp;
use crowntest::Query;
use crownui::prelude::{Cx, component};
use view::TaskId;

use crate::app::{provide_app_theme, provide_app_window_radius, shell, window_config};
use crate::fixtures::neutral_settings;
use crate::pristine::restore_config;
use crate::state::Store;
use crate::tasks::{Pump, threaded};

const FRAME_MS: u64 = 16;
const POLL: Duration = Duration::from_millis(8);
/// How long without a finished task counts as caught up.
const QUIET: Duration = Duration::from_millis(400);
const MOST_PUMPING: Duration = Duration::from_secs(20);
/// How long the lock screen's painter may go quiet between deliveries.
const PAINTER_QUIET: Duration = Duration::from_millis(3000);
/// The painter loads every system font first, which a debug build takes a while over.
const MOST_PAINTER_WAIT: Duration = Duration::from_secs(60);
/// Samples per side of the grid laid over the preview.
const PROBE_GRID: u32 = 16;
/// Distinct colors that tell a painted picture from a flat placeholder.
const PAINTED_COLORS: usize = 40;

/// A rectangle of the window, in pixels.
#[derive(Debug, Clone, Copy)]
pub struct Area {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}
/// Task ids handed out by the time any page has built.
const EARLY_TASKS: u64 = 256;

pub struct Window {
    pub app: CaptureApp,
    pump: Option<Pump>,
}

/// What a capture does to the store before the window builds over it.
pub trait Setup: Fn(&mut Cx, Store) + 'static {}
impl<F: Fn(&mut Cx, Store) + 'static> Setup for F {}

impl Window {
    /// The window on the page at `ordinal`, its tasks running as the app's would.
    pub fn open(ordinal: usize, setup: impl Setup) -> Result<Self> {
        let mut window = Self::held(ordinal, setup)?;
        let (executor, pump) = threaded()?;
        window.app.ui().set_task_executor(executor);
        window.pump = Some(pump);
        window.pump_until_quiet(QUIET);
        Ok(window)
    }

    /// The window with every task left unstarted, for answers the capture gives itself.
    pub fn held(ordinal: usize, setup: impl Setup) -> Result<Self> {
        restore_config()?;
        let root = component(move |cx| {
            let store = Store::load(cx);
            provide_app_theme(cx, store);
            provide_app_window_radius(cx, store);
            neutral_settings(cx, store);
            setup(cx, store);
            shell(cx, store, ordinal)
        });
        let app = capture::mount_app(root, &window_config(), None)?;
        Ok(Self { app, pump: None })
    }

    /// Answers every unstarted task that awaits a `T` with `answer`.
    pub fn answer<T: Clone + Send + 'static>(&mut self, answer: &T) {
        for id in 0..EARLY_TASKS {
            self.app
                .ui()
                .complete_task(TaskId(id), Box::new(answer.clone()));
        }
        self.app.settle();
    }

    /// Hands finished tasks back to the window until none has finished for `quiet`.
    pub fn pump_until_quiet(&mut self, quiet: Duration) {
        let Some(pump) = self.pump.as_ref() else {
            self.app.settle();
            return;
        };
        let started = Instant::now();
        let mut last_finished = started;
        while started.elapsed() < MOST_PUMPING && last_finished.elapsed() < quiet {
            let finished: Vec<_> = pump.drain().collect();
            if !finished.is_empty() {
                last_finished = Instant::now();
            }
            for (id, output) in finished {
                self.app.ui().complete_task(id, output);
            }
            self.app.advance_ms(FRAME_MS);
            std::thread::sleep(POLL);
        }
        self.app.settle();
    }

    /// Waits until the lock screen preview inside `area` shows a painted
    /// picture rather than its flat placeholder, then for the painter to go quiet.
    pub fn await_painter(&mut self, area: Area) {
        let started = Instant::now();
        while started.elapsed() < MOST_PAINTER_WAIT && !self.painted(area) {
            self.pump_until_quiet(PAINTER_QUIET);
        }
        self.pump_until_quiet(PAINTER_QUIET);
    }

    fn painted(&mut self, area: Area) -> bool {
        let Ok(pixels) = self.app.inline().renderer().read_pixels() else {
            return true;
        };
        let samples =
            (0..PROBE_GRID).flat_map(|row| (0..PROBE_GRID).map(move |column| (row, column)));
        let colors: HashSet<[u8; 3]> = samples
            .filter_map(|(row, column)| {
                let x = area.left + (area.right - area.left) * column / PROBE_GRID;
                let y = area.top + (area.bottom - area.top) * row / PROBE_GRID;
                let at = ((y * pixels.width + x) * 4) as usize;
                let rgb = pixels.rgba.get(at..at + 3)?;
                Some([rgb[0] >> 3, rgb[1] >> 3, rgb[2] >> 3])
            })
            .collect();
        colors.len() >= PAINTED_COLORS
    }

    /// Clicks `query` when it is on screen; says whether it was.
    pub fn click(&mut self, query: Query) -> bool {
        if !self.app.exists(query.clone()) {
            eprintln!(
                "design_capture: nothing matches {query:?}\n{}",
                self.app.semantics_snapshot()
            );
            return false;
        }
        self.app.click(query);
        self.pump_until_quiet(QUIET);
        true
    }

    /// Scrolls the page named `title` by `dy`.
    pub fn scroll(&mut self, query: Query, dy: f32) {
        if self.app.exists(query.clone()) {
            self.app.scroll(query, dy);
            self.app.settle();
        }
    }

    /// Types `keys` one press at a time, as a person does.
    pub fn press(&mut self, keys: &str) {
        for key in keys.chars() {
            self.app.press_key(&key.to_string());
        }
        self.app.settle();
    }
}
