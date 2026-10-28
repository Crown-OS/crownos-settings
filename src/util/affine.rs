use crownui::ext::Transform;
use crownui::prelude::Point;

/// The affine map taking each corner of the triangle `from` to the same
/// corner of `to`, or `None` when `from` is flat.
pub fn between(from: [Point; 3], to: [Point; 3]) -> Option<Transform> {
    corner_frame(from)
        .invert()
        .map(|into| into.then(corner_frame(to)))
}

/// Takes the unit triangle onto `triangle`, its first corner the origin's.
fn corner_frame([origin, first, second]: [Point; 3]) -> Transform {
    Transform {
        matrix: [
            first.x - origin.x,
            first.y - origin.y,
            second.x - origin.x,
            second.y - origin.y,
            origin.x,
            origin.y,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_corner_lands_on_its_partner() {
        let from = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(0.0, 5.0),
        ];
        let to = [
            Point::new(3.0, 4.0),
            Point::new(13.0, 8.0),
            Point::new(1.0, 9.0),
        ];
        let map = between(from, to).expect("not flat");
        for (corner, target) in from.into_iter().zip(to) {
            let landed = map.apply(corner);
            assert!((landed.x - target.x).abs() < 1e-4 && (landed.y - target.y).abs() < 1e-4);
        }
    }

    #[test]
    fn a_flat_triangle_has_no_map() {
        let flat = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(2.0, 2.0),
        ];
        assert_eq!(between(flat, flat), None);
    }
}
