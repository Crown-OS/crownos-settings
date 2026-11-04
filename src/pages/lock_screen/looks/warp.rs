//! Named bends of the clock's box, as the warp handles would leave it.

use crownos_config::schema::ClockFrame;

use crate::controls::Options;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Warp {
    Flat,
    /// Leaning forward, like italics.
    Lean,
    /// Narrowing toward the top, as if looked up at.
    Rise,
    /// Shrinking toward the right, as if turned away.
    Turn,
}

pub static WARPS: Options<Warp> = Options::new(&[
    ("None", Warp::Flat),
    ("Lean", Warp::Lean),
    ("Rise", Warp::Rise),
    ("Turn", Warp::Turn),
]);

impl Warp {
    /// How far each corner moves — top left, top right, bottom right, bottom
    /// left — as a share of the box.
    pub fn corners(self) -> [[f32; 2]; 4] {
        match self {
            Self::Flat => ClockFrame::default().warp,
            Self::Lean => [[0.14, 0.0], [0.14, 0.0], [0.0; 2], [0.0; 2]],
            Self::Rise => [[0.12, 0.0], [-0.12, 0.0], [0.0; 2], [0.0; 2]],
            Self::Turn => [[0.0; 2], [0.0, 0.1], [0.0, -0.1], [0.0; 2]],
        }
    }

    /// The named bend nearest `corners`.
    pub fn nearest(corners: [[f32; 2]; 4]) -> Self {
        let custom = |warp: &Self| {
            warp.corners()
                .iter()
                .flatten()
                .zip(corners.iter().flatten())
                .map(|(left, right)| (left - right).abs())
                .sum::<f32>()
        };
        WARPS
            .values()
            .min_by(|left, right| custom(left).total_cmp(&custom(right)))
            .unwrap_or(Self::Flat)
    }
}
