//! The lock screen as the preview plays it: a [`LockFlow`] driven by buttons
//! instead of a keyboard and a password check.

use std::time::{Duration, Instant};

use crownos_config::schema::Lock;
use lockscreen::flow::{AuthOutcome, Frame, KeyInput, LockFlow};

use crate::pages::lock_screen::moment::Moment;

/// The longest a transition may run, as the schema caps it: a flow started
/// this long ago has finished every one.
const LONGEST_TRANSITION: Duration = Duration::from_millis(2000);

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

    fn other(self) -> Self {
        match self {
            Self::Clock => Self::Password,
            Self::Password => Self::Clock,
        }
    }
}

pub struct Playback {
    flow: LockFlow,
    mode: Mode,
    /// Bumped by every request, so a delayed follow-up can tell it was
    /// overtaken.
    generation: u64,
}

impl Playback {
    pub fn new(lock: &Lock, now: Instant) -> Self {
        Self {
            flow: at_rest(lock, Mode::Clock, now),
            mode: Mode::Clock,
            generation: 0,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Animates to `mode` the way the lock screen does: a pointer move brings
    /// the prompt up, Escape sends it away.
    pub fn show(&mut self, lock: &Lock, mode: Mode, now: Instant) {
        self.generation += 1;
        if self.flow.is_unlocking() {
            self.flow = at_rest(lock, self.mode, now);
        }
        self.flow.configure(lock);
        if mode != self.mode {
            let _ = match mode {
                Mode::Password => self.flow.pointer(now),
                Mode::Clock => self.flow.key(KeyInput::Cancel, now),
            };
            self.mode = mode;
        }
    }

    pub fn play(&mut self, lock: &Lock, moment: Moment, now: Instant) {
        match moment {
            Moment::Appear => {
                self.generation += 1;
                self.flow = LockFlow::new(lock, now);
                self.mode = Mode::Clock;
            }
            Moment::Reveal => self.show(lock, self.mode.other(), now),
            Moment::Dismiss => {
                self.generation += 1;
                self.flow = at_rest(lock, self.mode, now);
                self.flow.finish(AuthOutcome::Accepted, now);
            }
        }
    }

    pub fn frame(&self, now: Instant) -> Frame {
        self.flow.frame(now)
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        self.flow.is_animating(now)
    }

    /// Unlocked and done animating: only the desktop is left.
    pub fn is_unlocked(&self, now: Instant) -> bool {
        self.flow.is_finished(now)
    }
}

/// A flow that has finished arriving, on `mode`, with nothing left to animate.
fn at_rest(lock: &Lock, mode: Mode, now: Instant) -> LockFlow {
    let Some(past) = now.checked_sub(LONGEST_TRANSITION) else {
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

    fn after(now: Instant, millis: u64) -> Instant {
        now + Duration::from_millis(millis)
    }

    #[test]
    fn at_rest_each_mode_shows_one_layer_and_nothing_moves() {
        let (lock, now) = (Lock::default(), Instant::now());
        let mut playback = Playback::new(&lock, now);
        let clock = playback.frame(now);
        assert_eq!(clock.clock, LayerStyle::REST);
        assert!(!clock.prompt.is_visible());
        assert_eq!(clock.backdrop, 1.0);
        assert!(!playback.is_animating(now));

        playback.show(&lock, Mode::Password, now);
        assert!(playback.is_animating(now));
        let later = after(now, 2001);
        assert_eq!(playback.frame(later).prompt, LayerStyle::REST);
        assert!(!playback.frame(later).clock.is_visible());
    }

    #[test]
    fn reveal_toggles_between_the_two_screens() {
        let (lock, now) = (Lock::default(), Instant::now());
        let mut playback = Playback::new(&lock, now);
        playback.play(&lock, Moment::Reveal, now);
        assert_eq!(playback.mode(), Mode::Password);
        playback.play(&lock, Moment::Reveal, after(now, 2001));
        assert_eq!(playback.mode(), Mode::Clock);
        assert!(playback.frame(after(now, 4002)).clock.is_visible());
    }

    #[test]
    fn dismiss_leaves_only_the_desktop_and_appear_brings_the_lock_back() {
        let (lock, now) = (Lock::default(), Instant::now());
        let mut playback = Playback::new(&lock, now);
        let before = playback.generation();
        playback.play(&lock, Moment::Dismiss, now);
        assert_ne!(playback.generation(), before);
        assert!(!playback.is_unlocked(now));
        let gone = after(now, 2001);
        assert!(playback.is_unlocked(gone));
        let frame = playback.frame(gone);
        assert_eq!(frame.backdrop, 0.0);
        assert!(!frame.clock.is_visible() && !frame.prompt.is_visible());

        playback.play(&lock, Moment::Appear, gone);
        assert!(!playback.is_unlocked(gone));
        assert_eq!(playback.frame(after(gone, 2001)).clock, LayerStyle::REST);
    }
}
