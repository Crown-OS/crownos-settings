//! What the studio is asked for, and what it answers.

use crownos_config::schema::{ClockFrame, LockBackground};
use lockscreen::layout::{Layer, Part, Rect};
use lockscreen::lockscene::LockScene;
use lockscreen::tiny_skia::Pixmap;

use super::thumbnails::{Shelf, ThumbnailRequest};
use crate::util::pixmap::straight_rgba;

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutRequest {
    pub scene: LockScene,
    /// The output's logical size, which everything is laid out over.
    pub output: [f32; 2],
    /// Preview pixels per logical output pixel.
    pub scale: f32,
    /// A drag's look, laid out while it lasts: the glass keeps the frost it
    /// has instead of blurring the background again for every glyph height.
    pub draft: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackdropRequest {
    /// The desktop wallpaper's path; empty for none.
    pub desktop: String,
    pub background: LockBackground,
    /// The preview's size in pixels.
    pub pixels: [u32; 2],
    /// The output's size, which blurs are proportioned to.
    pub output: [u32; 2],
    /// Whether the background's subject stands in front of the clock.
    pub depth: bool,
    /// The lock's dim, which the subject stands in front of and so carries.
    pub dim: f32,
}

pub enum StudioCommand {
    Layout(Box<LayoutRequest>),
    /// Where the canvas shows the clock now, for its glass to be frosted
    /// with the background it stands over there.
    Frost(ClockFrame),
    Backdrop(BackdropRequest),
    Thumbnails(Box<ThumbnailRequest>),
}

/// One rasterised piece of the layout, premultiplied, at the request's scale.
pub struct Painted {
    /// Steady across layouts for the same place, as the lock screen's own.
    pub id: u64,
    pub layer: Layer,
    pub part: Option<Part>,
    /// The clock's date, which moves with the clock's frame.
    pub date: bool,
    /// Logical output pixels.
    pub rect: Rect,
    /// Logical output pixels of transparent room around `rect`.
    pub pad: f32,
    /// The character, for a clock glyph.
    pub glyph: Option<char>,
    /// `None` when the canvas already has this sprite's picture.
    pub pixmap: Option<Pixmap>,
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
    /// The lock's background, blurred as it is behind the prompt.
    Blurred,
    /// The background's subject, in front of the clock.
    Subject,
}

impl Slot {
    pub const ALL: [Self; 4] = [Self::Desktop, Self::Lock, Self::Blurred, Self::Subject];
}

/// How far the depth effect has got with the lock's background.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DepthState {
    #[default]
    Off,
    /// A flat color has no subject to cut out.
    NoPicture,
    /// The helper is cutting the subject out.
    Cutting,
    Ready,
    /// Cut, but held back: it would hide too much of the clock.
    HeldBack,
    /// No subject could be cut from this picture.
    Unavailable,
}

pub enum StudioEvent {
    Sprites {
        /// Which layout these are, counting from the studio's first.
        layout: u64,
        scale: f32,
        /// What the clock was laid out with; the canvas draws any edit made
        /// since as the difference.
        frame: ClockFrame,
        sprites: Vec<Painted>,
    },
    /// The glass faces of `layout` frosted again for where the clock is
    /// shown now, by sprite id.
    Frosted {
        layout: u64,
        faces: Vec<(u64, Pixmap)>,
    },
    Picture(Slot, Option<Picture>),
    PictureUnreadable(bool),
    Depth(DepthState),
    /// One picture per scene asked for, `None` where nothing could be drawn.
    Thumbnails(Shelf, Vec<Option<Picture>>),
}
