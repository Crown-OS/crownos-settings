//! The clock as the editor sees it: where its glyphs and date land once an
//! edit that is not laid out yet applies, and what a drag on a handle does
//! to it. Logical output pixels throughout.

use crownos_config::schema::{ClockFrame, ClockLook};
use lockscreen::clock::Homography;
use lockscreen::layout::{ClockGroup, Rect};

use super::{EditMode, Handle};

/// The clock laid out with one frame, shown with another.
pub struct ClockEdit {
    laid: ClockFrame,
    target: ClockFrame,
    /// The laid-out clock's box.
    laid_box: Rect,
    /// The shown clock's box, before its warp.
    shown_box: Rect,
    hours: Option<Rect>,
    minutes: Option<Rect>,
    /// The shown date, which moves and scales with the clock but never warps.
    date: Option<Rect>,
    warp: Homography,
}

impl ClockEdit {
    /// `None` when there is no clock.
    pub fn new(
        laid: ClockFrame,
        target: ClockFrame,
        glyphs: impl Iterator<Item = (ClockGroup, Rect)> + Clone,
        date: Option<Rect>,
    ) -> Option<Self> {
        let laid_box = bounds(glyphs.clone().map(|(_, rect)| rect))?;
        let mut edit = Self {
            laid,
            target,
            laid_box,
            shown_box: laid_box,
            hours: None,
            minutes: None,
            date: None,
            warp: Homography::IDENTITY,
        };
        let shown = |wanted: ClockGroup| {
            bounds(
                glyphs
                    .clone()
                    .filter(move |(group, _)| *group == wanted)
                    .map(|(group, rect)| edit.moved(group, rect)),
            )
        };
        let (hours, minutes) = (shown(ClockGroup::Hours), shown(ClockGroup::Minutes));
        let shown_box = bounds(glyphs.map(|(group, rect)| edit.moved(group, rect)))?;
        edit.date = date.map(|rect| edit.moved(ClockGroup::Separator, rect));
        edit.hours = hours;
        edit.minutes = minutes;
        edit.shown_box = shown_box;
        edit.warp = warp_of(shown_box, target.warp).unwrap_or(Homography::IDENTITY);
        Some(edit)
    }

    fn scale(&self) -> f32 {
        self.target.scale / self.laid.scale.max(f32::EPSILON)
    }

    fn group_shift(frame: &ClockFrame, group: ClockGroup) -> [f32; 2] {
        match group {
            ClockGroup::Hours => frame.hours,
            ClockGroup::Minutes => frame.minutes,
            ClockGroup::Separator => [0.0; 2],
        }
    }

    /// The move and scale a glyph of `group` takes on its way from the laid
    /// frame to the shown one, before the warp.
    fn moving(&self, group: ClockGroup) -> Homography {
        let scale = self.scale();
        let centre = self.laid_box.centre();
        let (laid, shown) = (
            Self::group_shift(&self.laid, group),
            Self::group_shift(&self.target, group),
        );
        let shift: [f32; 2] = std::array::from_fn(|axis| {
            centre[axis] * (1.0 - scale) - laid[axis] * scale
                + shown[axis]
                + self.target.offset[axis]
                - self.laid.offset[axis]
        });
        Homography::affine([scale; 2], shift)
    }

    fn moved(&self, group: ClockGroup, rect: Rect) -> Rect {
        let moving = self.moving(group);
        let [x, y] = moving.apply([rect.x, rect.y]).unwrap_or([rect.x, rect.y]);
        let scale = self.scale();
        Rect {
            x,
            y,
            width: rect.width * scale,
            height: rect.height * scale,
        }
    }

    /// Where a glyph of `group`, laid out, is drawn now.
    pub fn map(&self, group: ClockGroup) -> Homography {
        self.warp.after(self.moving(group))
    }

    /// Where the laid-out date is drawn now.
    pub fn date_map(&self) -> Homography {
        self.moving(ClockGroup::Separator)
    }

    pub fn is_identity(&self) -> bool {
        self.laid == self.target
    }

    /// Whether the shown clock is bent, beyond moving and scaling.
    pub fn is_warped(&self) -> bool {
        self.target.is_warped()
    }

    /// The shown clock's four corners: top left, top right, bottom right,
    /// bottom left.
    pub fn corners(&self) -> [[f32; 2]; 4] {
        corners(self.shown_box).map(|corner| self.warp.apply(corner).unwrap_or(corner))
    }

    /// The box `group` is drawn in, warped, as four corners.
    pub fn group_corners(&self, group: ClockGroup) -> Option<[[f32; 2]; 4]> {
        let rect = match group {
            ClockGroup::Hours => self.hours?,
            ClockGroup::Minutes => self.minutes?,
            ClockGroup::Separator => return None,
        };
        Some(corners(rect).map(|corner| self.warp.apply(corner).unwrap_or(corner)))
    }

    /// Around the shown digits and date, `pad` clear of them.
    pub fn outline(&self, pad: f32) -> Rect {
        let digits = bounds(self.corners().into_iter().map(|[x, y]| Rect {
            x,
            y,
            ..Rect::default()
        }))
        .unwrap_or(self.shown_box);
        let group = bounds([digits].into_iter().chain(self.date)).unwrap_or(digits);
        Rect {
            x: group.x - pad,
            y: group.y - pad,
            width: group.width + 2.0 * pad,
            height: group.height + 2.0 * pad,
        }
    }

    /// The handle under `point` in `mode`; `reach` is how near a corner
    /// counts as on it, and `pad` how far the outline stands off the clock.
    pub fn hit(&self, mode: EditMode, point: [f32; 2], reach: f32, pad: f32) -> Option<Handle> {
        let flat = self.warp.inverse()?.apply(point)?;
        let inside = |rect: Option<Rect>| rect.is_some_and(|rect| contains(rect, flat));
        match mode {
            EditMode::Move => {
                let outline = self.outline(pad);
                let corner = [outline.x + outline.width, outline.y + outline.height];
                if distance(corner, point) <= reach {
                    Some(Handle::Stretch)
                } else {
                    contains(outline, point).then_some(Handle::Body)
                }
            }
            EditMode::Warp => self
                .corners()
                .iter()
                .position(|corner| distance(*corner, point) <= reach)
                .map(Handle::Corner)
                .or(inside(Some(self.shown_box)).then_some(Handle::Body)),
            EditMode::Split => [ClockGroup::Hours, ClockGroup::Minutes]
                .into_iter()
                .find(|group| match group {
                    ClockGroup::Hours => inside(self.hours),
                    _ => inside(self.minutes),
                })
                .map(Handle::Group),
        }
    }

    /// The shown clock's width and height, before its warp.
    pub fn extent(&self) -> [f32; 2] {
        [self.shown_box.width, self.shown_box.height]
    }
}

/// `start` once `handle` is dragged by `moved`, on a clock `extent` big when
/// the drag began; a sideways pull on the stretch handle past `colon_pull`
/// adds the colon to the right and takes it away to the left.
pub fn dragged(
    handle: Handle,
    start: &ClockLook,
    extent: [f32; 2],
    moved: [f32; 2],
    colon_pull: f32,
) -> ClockLook {
    let mut look = start.clone();
    let frame = &mut look.frame;
    let add = |base: [f32; 2]| [base[0] + moved[0], base[1] + moved[1]];
    match handle {
        Handle::Body => frame.offset = add(start.frame.offset),
        Handle::Group(ClockGroup::Hours) => frame.hours = add(start.frame.hours),
        Handle::Group(ClockGroup::Minutes) => frame.minutes = add(start.frame.minutes),
        Handle::Group(ClockGroup::Separator) | Handle::Wheel => {}
        Handle::Stretch => {
            let grown = 1.0 + moved[1] / extent[1].max(1.0);
            look.stretch = (start.clamped_stretch() * grown)
                .clamp(ClockLook::MIN_STRETCH, ClockLook::MAX_STRETCH);
            look.colon = match moved[0] {
                pulled if pulled > colon_pull => true,
                pulled if pulled < -colon_pull => false,
                _ => start.colon,
            };
        }
        Handle::Corner(index) => {
            frame.warp[index] = std::array::from_fn(|axis| {
                (start.frame.warp[index][axis] + moved[axis] / extent[axis].max(1.0))
                    .clamp(-ClockFrame::MAX_WARP, ClockFrame::MAX_WARP)
            });
        }
    }
    look
}

fn warp_of(clock: Rect, warp: [[f32; 2]; 4]) -> Option<Homography> {
    let quad: [[f32; 2]; 4] = std::array::from_fn(|index| {
        let ([x, y], [dx, dy]) = (corners(clock)[index], warp[index]);
        [x + dx * clock.width, y + dy * clock.height]
    });
    Homography::box_to_quad([clock.x, clock.y], [clock.width, clock.height], quad)
}

pub fn corners(rect: Rect) -> [[f32; 2]; 4] {
    [
        [rect.x, rect.y],
        [rect.x + rect.width, rect.y],
        [rect.x + rect.width, rect.y + rect.height],
        [rect.x, rect.y + rect.height],
    ]
}

fn contains(rect: Rect, [x, y]: [f32; 2]) -> bool {
    x >= rect.x && y >= rect.y && x <= rect.x + rect.width && y <= rect.y + rect.height
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn bounds(rects: impl Iterator<Item = Rect>) -> Option<Rect> {
    let [left, top, right, bottom] = rects.fold(None, |bounds: Option<[f32; 4]>, rect| {
        let [l, t, r, b] = bounds.unwrap_or([f32::MAX, f32::MAX, f32::MIN, f32::MIN]);
        Some([
            l.min(rect.x),
            t.min(rect.y),
            r.max(rect.x + rect.width),
            b.max(rect.y + rect.height),
        ])
    })?;
    Some(Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATE: Rect = Rect {
        x: 150.0,
        y: 60.0,
        width: 100.0,
        height: 20.0,
    };

    fn glyphs() -> impl Iterator<Item = (ClockGroup, Rect)> + Clone {
        let cell = |x: f32| Rect {
            x,
            y: 100.0,
            width: 50.0,
            height: 100.0,
        };
        [
            (ClockGroup::Hours, cell(100.0)),
            (ClockGroup::Hours, cell(150.0)),
            (ClockGroup::Separator, cell(200.0)),
            (ClockGroup::Minutes, cell(250.0)),
            (ClockGroup::Minutes, cell(300.0)),
        ]
        .into_iter()
    }

    fn unedited() -> ClockEdit {
        let frame = ClockFrame::default();
        ClockEdit::new(frame, frame, glyphs(), Some(DATE)).expect("a clock")
    }

    #[test]
    fn an_unedited_clock_draws_where_it_was_laid_out() {
        let edit = unedited();
        assert!(edit.is_identity());
        assert_eq!(edit.corners()[0], [100.0, 100.0]);
        assert_eq!(
            edit.map(ClockGroup::Hours).apply([120.0, 130.0]),
            Some([120.0, 130.0])
        );
        let outline = edit.outline(5.0);
        assert_eq!([outline.x, outline.y], [95.0, 55.0], "the date is inside");
        assert_eq!([outline.width, outline.height], [260.0, 150.0]);
    }

    #[test]
    fn a_press_finds_the_right_handle_in_each_mode() {
        let edit = unedited();
        assert_eq!(
            edit.hit(EditMode::Move, [352.0, 203.0], 8.0, 5.0),
            Some(Handle::Stretch)
        );
        assert_eq!(
            edit.hit(EditMode::Move, [200.0, 70.0], 8.0, 5.0),
            Some(Handle::Body),
            "the date drags the clock too"
        );
        assert_eq!(
            edit.hit(EditMode::Warp, [349.0, 199.0], 8.0, 5.0),
            Some(Handle::Corner(2))
        );
        assert_eq!(
            edit.hit(EditMode::Split, [310.0, 150.0], 8.0, 5.0),
            Some(Handle::Group(ClockGroup::Minutes))
        );
        assert_eq!(edit.hit(EditMode::Move, [10.0, 10.0], 8.0, 5.0), None);
    }

    #[test]
    fn dragging_edits_what_the_handle_holds() {
        let edit = unedited();
        let look = ClockLook::default();
        let moved = dragged(Handle::Body, &look, edit.extent(), [10.0, -5.0], 30.0);
        assert_eq!(moved.frame.offset, [10.0, -5.0]);
        let pulled = dragged(Handle::Corner(1), &look, edit.extent(), [25.0, 10.0], 30.0);
        assert_eq!(pulled.frame.warp[1], [0.1, 0.1]);
        let split = dragged(
            Handle::Group(ClockGroup::Minutes),
            &look,
            edit.extent(),
            [0.0, 40.0],
            30.0,
        );
        assert_eq!(split.frame.minutes, [0.0, 40.0]);
    }

    #[test]
    fn the_corner_stretches_down_and_pulls_the_colon_sideways() {
        let edit = unedited();
        let look = ClockLook {
            stretch: 2.0,
            ..ClockLook::default()
        };
        let taller = dragged(Handle::Stretch, &look, edit.extent(), [5.0, 50.0], 30.0);
        assert_eq!(taller.stretch, 3.0);
        assert!(taller.colon, "a small sideways wobble keeps the colon");
        let without = dragged(Handle::Stretch, &look, edit.extent(), [-40.0, 0.0], 30.0);
        assert!(!without.colon);
        assert_eq!(without.stretch, 2.0);
        let back = dragged(Handle::Stretch, &without, edit.extent(), [40.0, 0.0], 30.0);
        assert!(back.colon);
        let squashed = dragged(Handle::Stretch, &look, edit.extent(), [0.0, -100.0], 30.0);
        assert_eq!(squashed.stretch, ClockLook::MIN_STRETCH);
    }

    #[test]
    fn an_edit_not_laid_out_yet_moves_the_glyphs_and_the_date() {
        let laid = ClockFrame::default();
        let target = ClockFrame {
            offset: [20.0, 0.0],
            minutes: [0.0, 30.0],
            ..laid
        };
        let edit = ClockEdit::new(laid, target, glyphs(), Some(DATE)).expect("a clock");
        assert_eq!(
            edit.map(ClockGroup::Minutes).apply([250.0, 100.0]),
            Some([270.0, 130.0])
        );
        assert_eq!(
            edit.map(ClockGroup::Hours).apply([100.0, 100.0]),
            Some([120.0, 100.0])
        );
        assert_eq!(edit.date_map().apply([150.0, 60.0]), Some([170.0, 60.0]));
    }
}
