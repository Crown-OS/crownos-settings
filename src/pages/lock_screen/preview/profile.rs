//! What dragging the preview's clock costs, with the studio on a thread of
//! its own as in the app and frames paced at 60 Hz: the UI thread's time for
//! the studio's answers, for each pointer event and for each frame, and how
//! many frames a stretch trails the pointer.
//!
//! `CROWN_CONFIG_DIR=<scratch dir> CROWNSETTINGS_RENDER_WALLPAPER=<picture>
//! cargo test --release profile_clock_drag -- --ignored --nocapture`

use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use crownos_config::schema::lock::ClockKey;
use crownos_config::schema::{ClockFont, ClockFrame, ClockLook, appearance};
use crownui::ext::{InlineApp, TextSystem};
use crownui::kit::{INTER_FAMILY, inter_font};
use crownui::prelude::{Cx, Point, Size, Styled, Vector, component, vstack};
use impeller::{ImpellerBackend, Offscreen};
use lockscreen::layout::ClockGroup;
use taffy_layout::TaffyEngine;
use view::{TaskExecutor, TaskFuture, TaskId};

use super::editor::{EditMode, OUTLINE_PAD};
use super::{Preview, Sheet, State};
use crate::pages::lock_screen::editor;
use crate::state::Store;

const WIDTH: f32 = 760.0;
const HEIGHT: f32 = 1180.0;
const DEVICE_SCALE: f32 = 1.25;
const FRAME: Duration = Duration::from_micros(16_667);
const DRAG_STEPS: usize = 90;
/// Frames without an answer from the studio that count as it having caught up.
const QUIET_FRAMES: u32 = 30;
const MOST_SETTLE_FRAMES: u32 = 600;

type Finished = (TaskId, Box<dyn Any + Send>);

/// Runs `cx.spawn` futures on a tokio thread, as the app's runtime does.
struct ThreadedTasks {
    tasks: tokio::runtime::Handle,
    finished: Sender<Finished>,
}

impl TaskExecutor for ThreadedTasks {
    fn spawn(&mut self, id: TaskId, future: TaskFuture) {
        let finished = self.finished.clone();
        self.tasks.spawn(async move {
            let _ = finished.send((id, future.await));
        });
    }

    fn cancel(&mut self, _: TaskId) {}
}

fn task_thread() -> tokio::runtime::Handle {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a task runtime");
    let handle = runtime.handle().clone();
    std::thread::spawn(move || runtime.block_on(std::future::pending::<()>()));
    handle
}

#[derive(Default)]
struct Sample {
    answers: Vec<Duration>,
    events: Vec<Duration>,
    frames: Vec<Duration>,
    answered_frames: usize,
    /// Frames each shown stretch trailed the pointer by.
    trailing: Vec<usize>,
}

struct Profiler {
    app: InlineApp<ImpellerBackend, TaffyEngine>,
    state: State,
    finished: Receiver<Finished>,
    epoch: Instant,
    /// When the next display frame is due.
    next_frame: Instant,
}

impl Profiler {
    fn new(look: ClockLook, wallpaper: String) -> Self {
        let mounted = Rc::new(Cell::new(None));
        let mount = Rc::clone(&mounted);
        let root = component(move |cx| {
            let store = Store::defaults(cx);
            store.set(cx, appearance::Wallpaper, wallpaper.clone());
            store.set(cx, ClockKey, look.clone());
            let preview = Preview::start(cx, store);
            mount.set(Some(preview.state));
            vstack(editor(cx, store, &preview)).w(WIDTH).p(24.0)
        });
        let surface = Offscreen {
            width: (WIDTH * DEVICE_SCALE) as u32,
            height: (HEIGHT * DEVICE_SCALE) as u32,
            scale: DEVICE_SCALE,
        };
        let mut app = InlineApp::<ImpellerBackend, TaffyEngine>::with_surface(
            root,
            TaffyEngine::new(),
            Size::new(WIDTH, HEIGHT),
            DEVICE_SCALE,
            &surface,
        )
        .expect("an offscreen renderer");
        app.text_cache()
            .system_mut()
            .register_font(inter_font().to_vec(), Some(INTER_FAMILY))
            .expect("Inter registers");
        let (finished, receiver) = channel();
        app.ui().set_task_executor(Box::new(ThreadedTasks {
            tasks: task_thread(),
            finished,
        }));
        Self {
            app,
            state: mounted.get().expect("the preview mounts"),
            finished: receiver,
            epoch: Instant::now(),
            next_frame: Instant::now(),
        }
    }

    /// One display frame: the studio's answers, then `input`, then the frame.
    fn step(&mut self, sample: &mut Sample, input: impl FnOnce(&mut Cx, State)) {
        let begun = Instant::now();
        let mut answered = false;
        while let Ok((id, output)) = self.finished.try_recv() {
            self.app.ui().complete_task(id, output);
            answered = true;
        }
        let delivered = Instant::now();
        let cx = self.app.ui().cx();
        input(cx, self.state);
        let _ = cx.flush();
        let handled = Instant::now();
        self.app
            .frame_at(self.epoch.elapsed())
            .expect("a frame renders");
        let drawn = Instant::now();
        if answered {
            sample.answers.push(delivered - begun);
            sample.answered_frames += 1;
        }
        sample.events.push(handled - delivered);
        sample.frames.push(drawn - handled);
        self.next_frame = (self.next_frame + FRAME).max(drawn);
        std::thread::sleep(self.next_frame.saturating_duration_since(Instant::now()));
    }

    /// Frames until the studio has nothing more to say, and the clock is up.
    fn settle(&mut self, sample: &mut Sample) -> u32 {
        let (mut quiet, mut frames) = (0, 0);
        while quiet < QUIET_FRAMES && frames < MOST_SETTLE_FRAMES {
            let answered = sample.answered_frames;
            self.step(sample, |_, _| {});
            let laid = self.tallest_glyph().is_some();
            quiet = if sample.answered_frames == answered && laid {
                quiet + 1
            } else {
                0
            };
            frames += 1;
        }
        frames - quiet
    }

    fn tallest_glyph(&mut self) -> Option<f32> {
        let state = self.state;
        state.sprites.with_untracked(self.app.ui().cx(), |sprites| {
            sprites
                .clock_glyphs()
                .map(|(_, rect)| rect.height)
                .reduce(f32::max)
        })
    }

    /// Where on the canvas `pick` finds a handle of the clock as shown.
    fn handle_point(&mut self, pick: impl Fn(&super::editor::ClockEdit, f32) -> [f32; 2]) -> Point {
        let state = self.state;
        let cx = self.app.ui().cx();
        let scale = state.to_canvas(cx);
        let frame = state.shown_look(cx).frame;
        let edit = state.clock_edit(cx, frame).expect("a clock is laid out");
        let [x, y] = pick(&edit, scale);
        Point::new(x * scale, y * scale)
    }

    fn drag(&mut self, label: &str, at: Point, by: impl Fn(usize) -> Vector) {
        let mut sample = Sample::default();
        self.step(&mut sample, |cx, state| state.grab(cx, at));
        let state = self.state;
        let grabbed = state
            .grab
            .with_untracked(self.app.ui().cx(), |grab| grab.clone());
        let Some(grab) = grabbed else {
            println!("{label}: nothing grabbed at {at:?}");
            return;
        };
        let start_height = self.tallest_glyph().unwrap_or(1.0);
        let start_stretch = grab.look.clamped_stretch();
        let mut wanted = Vec::with_capacity(DRAG_STEPS);
        for step in 1..=DRAG_STEPS {
            let total = by(step);
            self.step(&mut sample, |cx, state| state.drag_by(cx, total));
            let look = state.grab.with_untracked(self.app.ui().cx(), |grab| {
                grab.as_ref().map(|grab| grab.look.clamped_stretch())
            });
            wanted.push(look.unwrap_or(start_stretch));
            let shown = start_stretch * self.tallest_glyph().unwrap_or(start_height) / start_height;
            if wanted.last() != wanted.first() {
                let caught = wanted
                    .iter()
                    .rposition(|stretch| *stretch <= shown * 1.005)
                    .unwrap_or(0);
                sample.trailing.push(wanted.len() - 1 - caught);
            }
        }
        self.step(&mut sample, |cx, state| state.release(cx));
        let release = Instant::now();
        let caught_up = self.settle(&mut Sample::default());
        sample.report(label, caught_up, release.elapsed());
    }
}

impl Sample {
    fn report(mut self, label: &str, caught_up: u32, settled: Duration) {
        let millis = |duration: &Duration| duration.as_secs_f64() * 1e3;
        let stats = |values: &mut Vec<Duration>| {
            if values.is_empty() {
                return "      -       -       -".to_owned();
            }
            values.sort_unstable();
            let mean = values.iter().map(millis).sum::<f64>() / values.len() as f64;
            let p95 = millis(&values[(values.len() - 1) * 95 / 100]);
            let max = millis(&values[values.len() - 1]);
            format!("{mean:>7.2} {p95:>7.2} {max:>7.2}")
        };
        let trailing = match self.trailing.is_empty() {
            true => "-".to_owned(),
            false => {
                let mean = self.trailing.iter().sum::<usize>() as f64 / self.trailing.len() as f64;
                let max = self.trailing.iter().max().copied().unwrap_or_default();
                format!("{mean:.1} (max {max})")
            }
        };
        println!(
            "{label:<10} event {} | answers {} x{:<3} | frame {} | stretch trails {trailing} frames | caught up {caught_up} frames ({:.0} ms)",
            stats(&mut self.events),
            stats(&mut self.answers),
            self.answered_frames,
            stats(&mut self.frames),
            millis(&settled),
        );
    }
}

/// The look the user's lock has: a stretched, frosted Grotesk clock.
fn glass_look() -> ClockLook {
    ClockLook {
        font: ClockFont::Grotesk,
        weight: 613.0,
        stretch: 2.0,
        whiteness: 0.31,
        shadow: 0.8,
        frame: ClockFrame {
            offset: [-18.0, 29.0],
            ..ClockFrame::default()
        },
        ..ClockLook::default()
    }
}

#[test]
#[ignore = "needs a GPU, a picture and a scratch CROWN_CONFIG_DIR; prints drag timings"]
fn profile_clock_drag() {
    let (Some(_), Ok(wallpaper)) = (
        std::env::var_os(crownos_config::CONFIG_DIR_ENV),
        std::env::var("CROWNSETTINGS_RENDER_WALLPAPER"),
    ) else {
        return;
    };
    println!("columns: mean p95 max ms; answers = UI time taking in the studio's sprites");
    let mut profiler = Profiler::new(glass_look(), wallpaper);
    let started = profiler.settle(&mut Sample::default());
    println!("first layout after {started} frames");

    let body = profiler.handle_point(|edit, _| edit.outline(0.0).centre());
    profiler.drag("move", body, |step| {
        Vector::new(step as f32 * 2.0, step as f32)
    });
    let corner = profiler.handle_point(|edit, scale| {
        let outline = edit.outline(OUTLINE_PAD / scale);
        [outline.x + outline.width, outline.y + outline.height]
    });
    profiler.drag("stretch", corner, |step| {
        Vector::new(0.0, step as f32 * 0.8)
    });
    let state = profiler.state;
    let cx = profiler.app.ui().cx();
    state.sheet.set(cx, Sheet::Effects);
    state.edit_mode.set(cx, EditMode::Warp);
    profiler.settle(&mut Sample::default());
    let corner = profiler.handle_point(|edit, _| edit.corners()[2]);
    profiler.drag("warp", corner, |step| {
        Vector::new(step as f32 * 0.6, step as f32 * 0.3)
    });
    let cx = profiler.app.ui().cx();
    state.edit_mode.set(cx, EditMode::Split);
    profiler.settle(&mut Sample::default());
    let minutes = profiler.handle_point(|edit, _| {
        let corners = edit
            .group_corners(ClockGroup::Minutes)
            .expect("minutes are laid out");
        [
            (corners[0][0] + corners[2][0]) / 2.0,
            (corners[0][1] + corners[2][1]) / 2.0,
        ]
    });
    profiler.drag("split", minutes, |step| Vector::new(0.0, step as f32));
    let cx = profiler.app.ui().cx();
    state.sheet.set(cx, Sheet::Clock);
    profiler.settle(&mut Sample::default());
    let mut sample = Sample::default();
    for _ in 0..DRAG_STEPS {
        profiler.step(&mut sample, |cx, state| state.zoom(cx, -4.0));
    }
    let release = Instant::now();
    let caught_up = profiler.settle(&mut Sample::default());
    sample.report("ctrl-wheel", caught_up, release.elapsed());
}
