//! The lock screen as the preview plays it: a [`LockFlow`] moved between the
//! clock and the password prompt by a switch instead of a keyboard, with the
//! lock's one animation.

use std::time::Instant;

use crownos_config::schema::Lock;
use lockscreen::flow::{Frame, KeyInput, LockFlow};
use lockscreen::transition::Spring;

/// Which screen the preview rests on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Clock,
    Password,
}

impl Mode {
    pub const ALL: [Self; 2] = [Self::Clock, Self::Password];

    pub fn title(self) -> &'static str {
        match self {
            Self::Clock => "Clock",
            Self::Password => "Password",
        }
    }
}

pub struct Playback {
    flow: LockFlow,
    mode: Mode,
}

impl Playback {
    pub fn new(lock: &Lock, now: Instant) -> Self {
        Self {
            flow: at_rest(lock, Mode::Clock, now),
            mode: Mode::Clock,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Animates to `mode` the way the lock screen does: a pointer move brings
    /// the prompt up, Escape sends it away.
    pub fn show(&mut self, lock: &Lock, mode: Mode, now: Instant) {
        self.flow.configure(lock);
        if mode == self.mode {
            return;
        }
        let _ = match mode {
            Mode::Password => self.flow.pointer(now),
            Mode::Clock => self.flow.key(KeyInput::Cancel, now),
        };
        self.mode = mode;
    }

    pub fn frame(&self, now: Instant) -> Frame {
        self.flow.frame(now)
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        self.flow.is_animating(now)
    }
}

/// A flow that has finished arriving, on `mode`, with nothing left to animate.
fn at_rest(lock: &Lock, mode: Mode, now: Instant) -> LockFlow {
    let Some(past) = now.checked_sub(Spring::LOCK.settle_time()) else {
        return LockFlow::settled(lock, now);
    };
    let mut flow = LockFlow::settled(lock, past);
    if mode == Mode::Password {
        let _ = flow.pointer(past);
    }
    flow
}

#[cfg(test)]
mod tests {
    use lockscreen::transition::LayerStyle;

    use super::*;

    #[test]
    fn at_rest_the_clock_shows_alone_and_nothing_moves() {
        let (lock, now) = (Lock::default(), Instant::now());
        let playback = Playback::new(&lock, now);
        let clock = playback.frame(now);
        assert_eq!(clock.clock, LayerStyle::REST);
        assert!(!clock.prompt.is_visible());
        assert_eq!(clock.wallpaper.opacity, 1.0);
        assert!(!playback.is_animating(now));
    }

    #[test]
    fn the_switch_plays_to_the_prompt_and_back() {
        let (lock, now) = (Lock::default(), Instant::now());
        let mut playback = Playback::new(&lock, now);
        playback.show(&lock, Mode::Password, now);
        assert_eq!(playback.mode(), Mode::Password);
        assert!(playback.is_animating(now));
        let landed = now + Spring::LOCK.settle_time();
        assert_eq!(playback.frame(landed).prompt, LayerStyle::REST);
        assert!(!playback.frame(landed).clock.is_visible());

        playback.show(&lock, Mode::Clock, landed);
        let back = landed + Spring::LOCK.settle_time();
        assert_eq!(playback.frame(back).clock, LayerStyle::REST);
        assert!(!playback.is_animating(back));
    }
}
