//! What the studio is asked for, and what it answers.

use crownos_config::schema::{ClockFrame, LockBackground};
use lockscreen::layout::{Layer, Part, Rect};
use lockscreen::lockscene::LockScene;
use lockscreen::tiny_skia::Pixmap;

use crate::util::pixmap::straight_rgba;

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutRequest {
    pub scene: LockScene,
    /// The output's logical size, which everything is laid out over.
    pub output: [f32; 2],
    /// Preview pixels per logical output pixel.
    pub scale: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackdropRequest {
    /// The desktop wallpaper's path; empty for none.
    pub desktop: String,
    pub background: LockBackground,
    /// The preview's size in pixels.
    pub pixels: [u32; 2],
    /// The output's size, which the blur is proportioned to.
    pub output: [u32; 2],
    pub blur: f32,
    /// The clock's depth cut-out, framed like the background.
    pub depth: Option<String>,
    /// The lock's dim, which the cut-out stands in front of and so carries.
    pub dim: f32,
}

pub enum StudioCommand {
    Layout(Box<LayoutRequest>),
    Backdrop(BackdropRequest),
}

/// One rasterised piece of the layout, premultiplied, at the request's scale.
pub struct Painted {
    /// Steady across layouts for the same place, as the lock screen's own.
    pub id: u64,
    pub layer: Layer,
    pub part: Option<Part>,
    /// Logical output pixels.
    pub rect: Rect,
    /// Logical output pixels of transparent room around `rect`.
    pub pad: f32,
    /// The character, for a clock glyph.
    pub glyph: Option<char>,
    pub pixmap: Pixmap,
}

/// Straight-alpha RGBA8, ready to upload.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Picture {
    pub(super) fn of(pixmap: &Pixmap) -> Self {
        Self {
            width: pixmap.width(),
            height: pixmap.height(),
            rgba: straight_rgba(pixmap),
        }
    }
}

/// The pictures the preview stacks with the lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// The desktop wallpaper, what is left once unlocked.
    Desktop,
    /// The lock's own picture, when it has one.
    Lock,
    /// The lock's background, blurred.
    Blurred,
    /// The background's subject, in front of the clock.
    Depth,
}

impl Slot {
    pub const ALL: [Self; 4] = [Self::Desktop, Self::Lock, Self::Blurred, Self::Depth];
}

pub enum StudioEvent {
    Sprites {
        scale: f32,
        /// What the clock was laid out with; the canvas draws any edit made
        /// since as the difference.
        frame: ClockFrame,
        sprites: Vec<Painted>,
    },
    Picture(Slot, Option<Picture>),
    PictureUnreadable(bool),
}
