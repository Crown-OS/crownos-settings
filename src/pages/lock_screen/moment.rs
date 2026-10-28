//! The three moments the lock screen animates.

use crownos_config::schema::{LockTransitions, Transition};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moment {
    Appear,
    Reveal,
    Dismiss,
}

impl Moment {
    pub const ALL: [Self; 3] = [Self::Appear, Self::Reveal, Self::Dismiss];

    pub fn title(self) -> &'static str {
        match self {
            Self::Appear => "Appear",
            Self::Reveal => "Reveal",
            Self::Dismiss => "Dismiss",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Self::Appear => "The clock arriving as the screen locks",
            Self::Reveal => "The clock giving way to the password prompt, and back after a while",
            Self::Dismiss => "Everything leaving once the password is accepted",
        }
    }

    pub fn of(self, transitions: &LockTransitions) -> &Transition {
        match self {
            Self::Appear => &transitions.appear,
            Self::Reveal => &transitions.reveal,
            Self::Dismiss => &transitions.dismiss,
        }
    }

    pub fn of_mut(self, transitions: &mut LockTransitions) -> &mut Transition {
        match self {
            Self::Appear => &mut transitions.appear,
            Self::Reveal => &mut transitions.reveal,
            Self::Dismiss => &mut transitions.dismiss,
        }
    }
}
