//! Handles on the preview's clock: drag it to move or scale it, pull its
//! corners to warp it, or move the hours and minutes apart.
//!
//! A drag edits a frame the canvas draws at once, by the difference from
//! the one the clock was laid out with; letting go writes it to the
//! settings, and the next layout catches up.

mod draw;
mod geometry;

use crownos_config::schema::lock::ClockKey;
use crownos_config::schema::{ClockFrame, Lock};
use crownui::prelude::{Cx, Point, Runtime, Vector};
use lockscreen::layout::ClockGroup;

use super::State;
use super::playback::Mode;
pub use draw::{handles, warped_image};
pub use geometry::ClockEdit;
use geometry::{GRAB_REACH, with_frame};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditMode {
    /// Drag the clock about; its corners scale it.
    #[default]
    Move,
    /// Each corner pulls on its own.
    Warp,
    /// The hours and the minutes move apart.
    Split,
}

impl EditMode {
    pub const ALL: [Self; 3] = [Self::Move, Self::Warp, Self::Split];

    pub fn title(self) -> &'static str {
        match self {
            Self::Move => "Move & scale",
            Self::Warp => "Warp",
            Self::Split => "Split",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    Body,
    Scale,
    /// Top left, top right, bottom right, bottom left.
    Corner(usize),
    Group(ClockGroup),
}

/// A drag under way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grab {
    handle: Handle,
    /// The frame when the drag began.
    start: ClockFrame,
    /// Where it began, in logical output pixels.
    from: [f32; 2],
    /// The frame as the drag has it now.
    pub frame: ClockFrame,
}

impl State {
    /// The frame the clock is shown with: the drag's, or the settings'.
    pub(super) fn shown_frame(self, runtime: &mut Runtime) -> ClockFrame {
        match self.grab.get(runtime) {
            Some(grab) => grab.frame,
            None => self.store.section(runtime, |lock: &Lock| lock.clock.frame),
        }
    }

    /// The clock as laid out, shown with `frame`.
    pub(super) fn clock_edit(self, runtime: &mut Runtime, frame: ClockFrame) -> Option<ClockEdit> {
        self.sprites.with(runtime, |sprites| {
            ClockEdit::new(sprites.frame, frame, sprites.clock_glyphs())
        })
    }

    /// Canvas pixels per logical output pixel.
    fn to_canvas(self, runtime: &mut Runtime) -> f32 {
        let width = self.size.with_untracked(runtime, |size| size.width);
        width / self.output(runtime)[0]
    }

    pub(super) fn grab(self, cx: &mut Cx, at: Point) {
        if self.mode.with_untracked(cx, |mode| *mode != Mode::Clock) {
            return;
        }
        let scale = self.to_canvas(cx);
        let start = cx.untrack(|runtime| self.shown_frame(runtime));
        let from = [at.x / scale, at.y / scale];
        let mode = self.edit_mode.with_untracked(cx, |mode| *mode);
        let handle = cx.untrack(|runtime| {
            self.clock_edit(runtime, start)?
                .hit(mode, from, GRAB_REACH / scale)
        });
        if let Some(handle) = handle {
            self.grab.set(
                cx,
                Some(Grab {
                    handle,
                    start,
                    from,
                    frame: start,
                }),
            );
        }
    }

    pub(super) fn drag_by(self, cx: &mut Cx, total: Vector) {
        let Some(grab) = self.grab.with_untracked(cx, |grab| *grab) else {
            return;
        };
        let scale = self.to_canvas(cx);
        let moved = [total.x / scale, total.y / scale];
        let frame = cx.untrack(|runtime| {
            self.clock_edit(runtime, grab.start)
                .map(|edit| edit.dragged(grab.handle, &grab.start, grab.from, moved))
        });
        if let Some(frame) = frame.filter(|frame| *frame != grab.frame) {
            self.grab.set(cx, Some(Grab { frame, ..grab }));
        }
    }

    pub(super) fn release(self, cx: &mut Cx) {
        let Some(grab) = self.grab.with_untracked(cx, |grab| *grab) else {
            return;
        };
        let look = cx.untrack(|runtime| self.store.get(runtime, ClockKey));
        self.store.set(cx, ClockKey, with_frame(&look, grab.frame));
        self.grab.set(cx, None);
    }
}
