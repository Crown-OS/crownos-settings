//! Whole clock looks to start from, and the faces the clock can wear.

use crownos_config::schema::{ClockArrangement, ClockFont, ClockLighting, ClockStyle, Lock};

use super::warp::Warp;

/// Close enough for a stored height to count as a preset's.
const STRETCH_TOLERANCE: f32 = 0.01;

/// Everything a look sets; the clock's place, its depth cut-out and its
/// animation stay as they are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub name: &'static str,
    pub style: ClockStyle,
    pub font: ClockFont,
    pub arrangement: ClockArrangement,
    pub stretch: f32,
    pub weight: Option<u16>,
    pub lighting: ClockLighting,
    pub glass: bool,
    pub shadow: f32,
    pub warp: Warp,
}

impl Look {
    const PLAIN: Self = Self {
        name: "",
        style: ClockStyle::Large,
        font: ClockFont::Tall,
        arrangement: ClockArrangement::Row,
        stretch: 2.4,
        weight: Some(200),
        lighting: ClockLighting::None,
        glass: false,
        shadow: 0.0,
        warp: Warp::Flat,
    };

    pub fn apply(&self, lock: &mut Lock) {
        lock.clock_style = self.style;
        let clock = &mut lock.clock;
        clock.font = self.font;
        clock.arrangement = self.arrangement;
        clock.stretch = self.stretch;
        clock.weight = self.weight;
        clock.lighting = self.lighting;
        clock.glass = self.glass;
        clock.shadow = self.shadow;
        clock.frame.warp = self.warp.corners();
    }

    pub fn is_worn_by(&self, lock: &Lock) -> bool {
        let clock = &lock.clock;
        lock.clock_style == self.style
            && clock.font == self.font
            && clock.arrangement == self.arrangement
            && (clock.stretch - self.stretch).abs() < STRETCH_TOLERANCE
            && clock.weight == self.weight
            && clock.lighting == self.lighting
            && clock.glass == self.glass
            && (clock.shadow - self.shadow).abs() < STRETCH_TOLERANCE
            && clock.frame.warp == self.warp.corners()
    }
}

pub static LOOKS: [Look; 8] = [
    Look {
        name: "Tower",
        arrangement: ClockArrangement::Stacked,
        stretch: 2.6,
        ..Look::PLAIN
    },
    Look {
        name: "Skyline",
        glass: true,
        ..Look::PLAIN
    },
    Look {
        name: "Didone",
        font: ClockFont::Serif,
        stretch: 2.6,
        weight: Some(500),
        ..Look::PLAIN
    },
    Look {
        name: "Aurora",
        stretch: 2.0,
        weight: Some(300),
        lighting: ClockLighting::Glow,
        ..Look::PLAIN
    },
    Look {
        name: "Glint",
        font: ClockFont::Narrow,
        stretch: 2.2,
        weight: Some(500),
        lighting: ClockLighting::Sheen,
        ..Look::PLAIN
    },
    Look {
        name: "Lean",
        font: ClockFont::Narrow,
        arrangement: ClockArrangement::Stacked,
        stretch: 1.8,
        weight: Some(600),
        shadow: 0.5,
        warp: Warp::Lean,
        ..Look::PLAIN
    },
    Look {
        name: "Horizon",
        font: ClockFont::Serif,
        arrangement: ClockArrangement::Stacked,
        stretch: 2.0,
        weight: Some(400),
        lighting: ClockLighting::Glow,
        warp: Warp::Rise,
        ..Look::PLAIN
    },
    Look {
        name: "Classic",
        style: ClockStyle::Classic,
        font: ClockFont::Theme,
        stretch: 1.0,
        weight: None,
        ..Look::PLAIN
    },
];

/// A typeface and arrangement the clock can wear, whatever else it looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Face {
    pub name: &'static str,
    pub font: ClockFont,
    pub arrangement: ClockArrangement,
}

impl Face {
    pub fn apply(&self, lock: &mut Lock) {
        lock.clock.font = self.font;
        lock.clock.arrangement = self.arrangement;
    }

    pub fn is_worn_by(&self, lock: &Lock) -> bool {
        lock.clock.font == self.font && lock.clock.arrangement == self.arrangement
    }
}

const fn face(name: &'static str, font: ClockFont, arrangement: ClockArrangement) -> Face {
    Face {
        name,
        font,
        arrangement,
    }
}

pub static FACES: [Face; 8] = [
    face("Tall", ClockFont::Tall, ClockArrangement::Row),
    face("Tall, stacked", ClockFont::Tall, ClockArrangement::Stacked),
    face("Narrow", ClockFont::Narrow, ClockArrangement::Row),
    face(
        "Narrow, stacked",
        ClockFont::Narrow,
        ClockArrangement::Stacked,
    ),
    face("Serif", ClockFont::Serif, ClockArrangement::Row),
    face(
        "Serif, stacked",
        ClockFont::Serif,
        ClockArrangement::Stacked,
    ),
    face("Theme font", ClockFont::Theme, ClockArrangement::Row),
    face(
        "Theme font, stacked",
        ClockFont::Theme,
        ClockArrangement::Stacked,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_look_is_worn_once_applied_and_only_it() {
        for look in &LOOKS {
            let mut lock = Lock::default();
            look.apply(&mut lock);
            let worn: Vec<_> = LOOKS
                .iter()
                .filter(|other| other.is_worn_by(&lock))
                .collect();
            assert_eq!(worn, [look]);
        }
    }

    #[test]
    fn applying_a_look_keeps_where_the_clock_was_moved() {
        let mut lock = Lock::default();
        lock.clock.frame.offset = [12.0, -4.0];
        lock.clock.morph_digits = false;
        LOOKS[5].apply(&mut lock);
        assert_eq!(lock.clock.frame.offset, [12.0, -4.0]);
        assert!(!lock.clock.morph_digits);
        assert_eq!(lock.clock.frame.warp, Warp::Lean.corners());
    }

    #[test]
    fn every_face_is_its_own() {
        for face in &FACES {
            let mut lock = Lock::default();
            face.apply(&mut lock);
            assert_eq!(
                FACES.iter().filter(|other| other.is_worn_by(&lock)).count(),
                1
            );
        }
    }
}
