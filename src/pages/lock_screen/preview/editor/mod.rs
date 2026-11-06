//! Handles on the preview's clock, after ColorOS's editor: drag the clock to
//! move it, pull the corner handle down to stretch it and sideways to add or
//! take away the colon, click it for the Clock & colour sheet. The Effects
//! sheet trades the handles for warp corners, or for the hours and minutes
//! moving apart.
//!
//! A drag edits a look the canvas draws at once, by the difference from the
//! one the clock was laid out with, while the studio lays the draft out
//! again if its glyphs changed; letting go writes it to the settings. A
//! scroll with Ctrl held is a drag too, let go once the wheel rests.

mod draw;
mod geometry;

use std::time::Duration;

use crownos_config::schema::lock::ClockKey;
use crownos_config::schema::{ClockFrame, ClockLook};
use crownui::prelude::{Cx, Point, Runtime, Vector};
use lockscreen::layout::ClockGroup;

use super::State;
use super::playback::Mode;
use super::toolbar::Sheet;
pub use draw::{handles, mapped_rect, warped_image};
use geometry::dragged;
pub use geometry::{ClockEdit, corners};

/// Canvas pixels: how near a handle a press grabs it.
const GRAB_REACH: f32 = 14.0;
/// Canvas pixels the outline stands off the clock.
pub const OUTLINE_PAD: f32 = 10.0;
/// Canvas pixels the stretch handle is pulled sideways to add or take away
/// the colon.
const COLON_PULL: f32 = 28.0;
/// How much a scroll with Ctrl held resizes the clock, per logical pixel.
const ZOOM_PER_PIXEL: f32 = 0.002;
/// How long the wheel rests before a resize is written to the settings.
const WHEEL_REST: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditMode {
    /// Drag the clock about; its corner handle stretches it.
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
            Self::Move => "Move & stretch",
            Self::Warp => "Warp",
            Self::Split => "Split",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    Body,
    Stretch,
    /// Top left, top right, bottom right, bottom left.
    Corner(usize),
    Group(ClockGroup),
    /// No handle: the clock's size, as Ctrl and the wheel change it.
    Wheel,
}

/// A drag under way.
#[derive(Debug, Clone, PartialEq)]
pub struct Grab {
    handle: Handle,
    /// The look when the drag began.
    start: ClockLook,
    /// The clock's width and height when the drag began.
    extent: [f32; 2],
    /// The look as the drag has it now.
    pub look: ClockLook,
}

impl State {
    /// The look the clock is shown with: the drag's, or the settings'.
    pub(super) fn shown_look(self, runtime: &mut Runtime) -> ClockLook {
        match self
            .grab
            .with(runtime, |grab| grab.as_ref().map(|grab| grab.look.clone()))
        {
            Some(look) => look,
            None => self.store.get(runtime, ClockKey),
        }
    }

    /// What dragging the clock does: the Effects sheet's choice while it is
    /// open, moving and stretching otherwise.
    pub(super) fn handle_mode(self, runtime: &mut Runtime) -> EditMode {
        match self.sheet.get(runtime) {
            Sheet::Effects => self.edit_mode.get(runtime),
            Sheet::Clock | Sheet::Wallpaper => EditMode::Move,
        }
    }

    /// The clock as laid out, shown with `frame`.
    pub(super) fn clock_edit(self, runtime: &mut Runtime, frame: ClockFrame) -> Option<ClockEdit> {
        self.sprites.with(runtime, |sprites| {
            ClockEdit::new(
                sprites.frame,
                frame,
                sprites.clock_glyphs(),
                sprites.clock_date(),
            )
        })
    }

    /// Canvas pixels per logical output pixel.
    pub(super) fn to_canvas(self, runtime: &mut Runtime) -> f32 {
        let width = self.size.with_untracked(runtime, |size| size.width);
        width / self.output(runtime)[0]
    }

    /// The handle under canvas point `at`, if the clock is up to be edited,
    /// and the clock's extent.
    fn handle_at(
        self,
        runtime: &mut Runtime,
        at: Point,
        start: &ClockLook,
    ) -> Option<(Handle, [f32; 2])> {
        if self
            .mode
            .with_untracked(runtime, |mode| *mode != Mode::Clock)
        {
            return None;
        }
        let scale = self.to_canvas(runtime);
        let mode = runtime.untrack(|runtime| self.handle_mode(runtime));
        runtime.untrack(|runtime| {
            let edit = self.clock_edit(runtime, start.frame)?;
            let point = [at.x / scale, at.y / scale];
            let handle = edit.hit(mode, point, GRAB_REACH / scale, OUTLINE_PAD / scale)?;
            Some((handle, edit.extent()))
        })
    }

    pub(super) fn grab(self, cx: &mut Cx, at: Point) {
        let start = cx.untrack(|runtime| self.shown_look(runtime));
        if let Some((handle, extent)) = self.handle_at(cx, at, &start) {
            self.grab.set(
                cx,
                Some(Grab {
                    handle,
                    look: start.clone(),
                    start,
                    extent,
                }),
            );
        }
    }

    pub(super) fn drag_by(self, cx: &mut Cx, total: Vector) {
        let Some(grab) = self.grab.with_untracked(cx, Clone::clone) else {
            return;
        };
        let scale = self.to_canvas(cx);
        let moved = [total.x / scale, total.y / scale];
        let look = dragged(
            grab.handle,
            &grab.start,
            grab.extent,
            moved,
            COLON_PULL / scale,
        );
        if look != grab.look {
            self.grab.set(cx, Some(Grab { look, ..grab }));
        }
    }

    pub(super) fn release(self, cx: &mut Cx) {
        let Some(grab) = self.grab.with_untracked(cx, Clone::clone) else {
            return;
        };
        self.store.set(cx, ClockKey, grab.look);
        self.grab.set(cx, None);
    }

    /// A click on the clock opens the Clock & colour sheet, as a tap does in
    /// ColorOS's editor.
    pub(super) fn click(self, cx: &mut Cx, at: Point) {
        let look = cx.untrack(|runtime| self.shown_look(runtime));
        let moving = cx.untrack(|runtime| self.handle_mode(runtime)) == EditMode::Move;
        if moving
            && matches!(
                self.handle_at(cx, at, &look),
                Some((Handle::Body | Handle::Stretch, _))
            )
        {
            self.sheet.set(cx, Sheet::Clock);
        }
    }

    /// Resizes the clock by a scroll of `pixels`, as Ctrl and the wheel over
    /// the preview do, writing the size to the settings once the wheel rests.
    pub(super) fn zoom(self, cx: &mut Cx, pixels: f32) {
        let held = self
            .grab
            .with_untracked(cx, |grab| grab.as_ref().map(|grab| grab.handle));
        if held.is_some_and(|handle| handle != Handle::Wheel) {
            return;
        }
        let mut look = cx.untrack(|runtime| self.shown_look(runtime));
        look.frame.scale = (look.frame.scale * (-pixels * ZOOM_PER_PIXEL).exp())
            .clamp(ClockFrame::MIN_SCALE, ClockFrame::MAX_SCALE);
        let resting = look.clone();
        self.grab.set(
            cx,
            Some(Grab {
                handle: Handle::Wheel,
                start: look.clone(),
                extent: [0.0; 2],
                look,
            }),
        );
        let rest = async { tokio::time::sleep(WHEEL_REST).await };
        cx.spawn(rest, move |cx, ()| {
            let rested = self.grab.with_untracked(cx, |grab| {
                grab.as_ref()
                    .is_some_and(|grab| grab.handle == Handle::Wheel && grab.look == resting)
            });
            if rested {
                self.release(cx);
            }
        });
    }
}
