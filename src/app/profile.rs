//! Frame times and allocations of the whole window, on Impeller's offscreen
//! target: every page opened from the sidebar, scrolled, and its first slider
//! dragged, with every backend populated.
//!
//! `CROWN_CONFIG_DIR=<scratch dir> cargo test --release profile_frames -- --ignored --nocapture`
//!
//! Slider drags persist their sections, so the profile refuses to run against
//! the real config directory.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crownos_config::CONFIG_DIR_ENV;
use crownos_config::schema::lock::Dim;
use crownos_config::schema::{appearance, display};
use crowntest::{Query, Role, role};
use crownui::ext::{InlineApp, TextSystem};
use crownui::kit::{INTER_FAMILY, inter_font};
use crownui::prelude::{Cx, Point, Rect, Size, Vector, component};
use impeller::{ImpellerBackend, Offscreen};
use semantics::{SemanticNode, UiAction};
use taffy_layout::TaffyEngine;

use super::fixtures::populate;
use super::root::shell;
use crate::pages;
use crate::state::Store;
use crate::util::alloc_count::{AllocationTally, live_bytes};

const WIDTH: f32 = 1040.0;
const HEIGHT: f32 = 720.0;
const FRAME: Duration = Duration::from_micros(16_667);
const MOST_FRAMES_PER_STEP: usize = 600;
const IDLE_FRAMES: usize = 120;
const SCROLL_STEPS: usize = 8;
const SCROLL_DELTA: f32 = 120.0;
const DRAG_STEPS: usize = 40;
const SLIDER_SEARCH_STEPS: usize = 16;
const NAVIGATION_CYCLES: usize = 3;
const EDIT_STEPS: usize = 60;

type OffscreenApp = InlineApp<ImpellerBackend, TaffyEngine>;

struct Profiler {
    app: OffscreenApp,
    store: Store,
    now: Duration,
}

#[derive(Default)]
struct Sample {
    frames: Vec<Duration>,
    allocations: AllocationTally,
}

impl Profiler {
    fn new(ordinal: usize) -> Self {
        let surface = Offscreen {
            width: WIDTH as u32,
            height: HEIGHT as u32,
            scale: 1.0,
        };
        let mounted = Rc::new(Cell::new(None));
        let mount = Rc::clone(&mounted);
        let root = component(move |cx| {
            let store = Store::defaults(cx);
            populate(cx, store);
            mount.set(Some(store));
            shell(cx, store, ordinal)
        });
        let mut app = OffscreenApp::with_surface(
            root,
            TaffyEngine::new(),
            Size::new(WIDTH, HEIGHT),
            1.0,
            &surface,
        )
        .expect("an offscreen renderer");
        app.text_cache()
            .system_mut()
            .register_font(inter_font().to_vec(), Some(INTER_FAMILY))
            .expect("Inter registers");
        let mut profiler = Self {
            app,
            store: mounted.get().expect("the root mounts"),
            now: Duration::ZERO,
        };
        profiler.settle(&mut Vec::new());
        profiler
    }

    fn frame(&mut self, frames: &mut Vec<Duration>) -> bool {
        self.now += FRAME;
        let started = Instant::now();
        let stats = self.app.frame_at(self.now).expect("a frame renders");
        frames.push(started.elapsed());
        stats.animating || self.app.ui().needs_frame() || self.app.ui().wants_animation_frame()
    }

    fn settle(&mut self, frames: &mut Vec<Duration>) {
        for _ in 0..MOST_FRAMES_PER_STEP {
            if !self.frame(frames) {
                return;
            }
        }
    }

    fn act(&mut self, query: Query, action: UiAction, frames: &mut Vec<Duration>) -> bool {
        let Ok(id) = query.find_one(self.app.ui().semantic_tree()) else {
            return false;
        };
        let performed = self.app.ui().perform(id, &action).is_ok();
        self.settle(frames);
        performed
    }

    fn scroll_page(&mut self, title: &str, dy: f32, frames: &mut Vec<Duration>) {
        let delta = Vector::new(0.0, dy);
        self.act(
            role(Role::ScrollView).name(title),
            UiAction::Scroll { delta },
            frames,
        );
    }

    fn visible_slider(&mut self) -> Option<Rect> {
        role(Role::Slider)
            .find_all(self.app.ui().semantic_tree())
            .into_iter()
            .map(|node| node.bounds)
            .find(|bounds| {
                bounds.size.width > 0.0 && bounds.min_y() > 0.0 && bounds.max_y() < HEIGHT
            })
    }

    fn reveal_slider(&mut self, title: &str) -> Option<Rect> {
        for _ in 0..SLIDER_SEARCH_STEPS {
            if let Some(bounds) = self.visible_slider() {
                return Some(bounds);
            }
            self.scroll_page(title, SCROLL_DELTA, &mut Vec::new());
        }
        None
    }

    fn drag_across(&mut self, track: Rect, frames: &mut Vec<Duration>) {
        let slider_at = |node: &&SemanticNode| node.bounds == track;
        for step in 0..=DRAG_STEPS {
            let x = track.min_x() + track.size.width * step as f32 / DRAG_STEPS as f32;
            let to = Point::new(x, track.center().y);
            let tree = self.app.ui().semantic_tree();
            let Some(id) = role(Role::Slider)
                .find_all(tree)
                .into_iter()
                .find(slider_at)
                .map(|node| node.id)
            else {
                return;
            };
            let _ = self.app.ui().perform(id, &UiAction::Drag { to });
            self.settle(frames);
        }
    }

    fn edit_each_frame(&mut self, frames: &mut Vec<Duration>, edit: impl Fn(&mut Cx, Store, f64)) {
        for step in 0..EDIT_STEPS {
            let store = self.store;
            edit(self.app.ui().cx(), store, step as f64 / EDIT_STEPS as f64);
            self.frame(frames);
        }
    }

    fn open(&mut self, title: &str, frames: &mut Vec<Duration>) {
        self.act(role(Role::MenuItem).name(title), UiAction::Click, frames);
    }

    fn measure(&mut self, label: &str, scenario: impl FnOnce(&mut Self, &mut Vec<Duration>)) {
        let before = AllocationTally::now();
        let mut sample = Sample::default();
        scenario(self, &mut sample.frames);
        sample.allocations = AllocationTally::now().since(before);
        sample.report(label);
    }
}

impl Sample {
    fn report(mut self, label: &str) {
        if self.frames.is_empty() {
            return;
        }
        self.frames.sort_unstable();
        let count = self.frames.len();
        let millis = |duration: Duration| duration.as_secs_f64() * 1e3;
        let percentile =
            |fraction: f64| millis(self.frames[((count - 1) as f64 * fraction) as usize]);
        let mean = millis(self.frames.iter().sum::<Duration>()) / count as f64;
        println!(
            "{label:<34} {count:>5} {mean:>7.2} {:>7.2} {:>7.2} {:>7.2} {:>9.0} {:>9.1}",
            percentile(0.5),
            percentile(0.95),
            percentile(1.0),
            self.allocations.count as f64 / count as f64,
            self.allocations.bytes as f64 / count as f64 / 1024.0,
        );
    }
}

#[test]
#[ignore = "needs a GPU and a scratch CROWN_CONFIG_DIR; prints a frame profile"]
fn profile_frames() {
    if std::env::var_os(CONFIG_DIR_ENV).is_none() {
        return;
    }
    println!(
        "{:<34} {:>5} {:>7} {:>7} {:>7} {:>7} {:>9} {:>9}",
        "scenario", "frames", "mean", "p50", "p95", "max", "allocs/f", "KiB/f"
    );
    let mut profiler = Profiler::new(pages::DEFAULT_PAGE);
    profiler.measure("idle", |profiler, frames| {
        (0..IDLE_FRAMES).for_each(|_| {
            profiler.frame(frames);
        });
    });
    for page in pages::all_pages() {
        let title = page.title;
        profiler.measure(&format!("open {title}"), |profiler, frames| {
            profiler.open(title, frames)
        });
        profiler.measure(&format!("scroll {title}"), |profiler, frames| {
            (0..SCROLL_STEPS).for_each(|_| profiler.scroll_page(title, SCROLL_DELTA, frames));
        });
        profiler.scroll_page(title, -SCROLL_DELTA * SCROLL_STEPS as f32, &mut Vec::new());
        if let Some(track) = profiler.reveal_slider(title) {
            profiler.measure(&format!("drag {title}"), |profiler, frames| {
                profiler.drag_across(track, frames);
            });
        }
    }
    profiler.open("Lock Screen", &mut Vec::new());
    profiler.measure("edit lock Dim on Lock Screen", |profiler, frames| {
        profiler.edit_each_frame(frames, |cx, store, value| store.set(cx, Dim, value as f32));
    });
    profiler.open("Appearance", &mut Vec::new());
    profiler.measure("edit Transparency on Appearance", |profiler, frames| {
        profiler.edit_each_frame(frames, |cx, store, value| {
            store.set(cx, appearance::Transparency, value);
        });
    });
    profiler.open("Display", &mut Vec::new());
    profiler.measure("edit Brightness on Display", |profiler, frames| {
        profiler.edit_each_frame(frames, |cx, store, value| {
            store.set(cx, display::Brightness, value * 100.0);
        });
    });
    profiler.measure("edit lock Dim on Display", |profiler, frames| {
        profiler.edit_each_frame(frames, |cx, store, value| store.set(cx, Dim, value as f32));
    });
    for cycle in 1..=NAVIGATION_CYCLES {
        pages::all_pages().for_each(|page| profiler.open(page.title, &mut Vec::new()));
        println!(
            "live heap after navigation cycle {cycle}: {} KiB",
            live_bytes() / 1024
        );
    }
}
