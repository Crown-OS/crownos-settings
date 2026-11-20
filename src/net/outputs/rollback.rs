//! The arrangement the worker puts back when nobody keeps the one just applied.

use std::time::Duration;

use tokio::time::Instant;

use super::{MonitorDraft, Outcome, REVERT_AFTER};

#[derive(Debug)]
pub struct Rollback {
    previous: Vec<MonitorDraft>,
    deadline: Instant,
}

impl Rollback {
    /// Only an arrangement the compositor applied can need putting back:
    /// after a refusal the old one is still in place.
    pub fn after_apply(
        previous: Vec<MonitorDraft>,
        outcome: &Outcome,
        now: Instant,
    ) -> Option<Self> {
        (*outcome == Outcome::Succeeded).then(|| Self {
            previous,
            deadline: now + REVERT_AFTER,
        })
    }

    /// Whole seconds until the arrangement goes back, rounded up.
    pub fn seconds_left(&self, now: Instant) -> u64 {
        let left = self.deadline.saturating_duration_since(now);
        left.as_secs() + u64::from(left.subsec_nanos() > 0)
    }

    /// When [`Self::seconds_left`] next drops by one. The last tick is the
    /// deadline.
    pub fn next_tick(&self, now: Instant) -> Instant {
        let seconds_after_tick = self.seconds_left(now).saturating_sub(1);
        self.deadline - Duration::from_secs(seconds_after_tick)
    }

    pub fn into_previous(self) -> Vec<MonitorDraft> {
        self.previous
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_apply_leaves_nothing_to_put_back() {
        let now = Instant::now();
        for refusal in [
            Outcome::Failed("head winit-1 has no such mode".into()),
            Outcome::Cancelled,
        ] {
            assert!(Rollback::after_apply(Vec::new(), &refusal, now).is_none());
        }
    }

    #[test]
    fn the_countdown_ticks_each_second_down_to_the_deadline() {
        let now = Instant::now();
        let Some(rollback) = Rollback::after_apply(Vec::new(), &Outcome::Succeeded, now) else {
            panic!("an applied arrangement must be put back unless kept");
        };
        let full = REVERT_AFTER.as_secs();
        assert_eq!(rollback.seconds_left(now), full);

        let mut tick = now;
        for expected in (0..full).rev() {
            let next = rollback.next_tick(tick);
            assert_eq!(next - tick, Duration::from_secs(1));
            assert_eq!(rollback.seconds_left(next), expected);
            tick = next;
        }
        assert_eq!(tick, now + REVERT_AFTER);
        assert_eq!(rollback.next_tick(tick + Duration::from_secs(3)), tick);
    }

    #[test]
    fn a_late_wake_still_counts_whole_seconds() {
        let now = Instant::now();
        let Some(rollback) = Rollback::after_apply(Vec::new(), &Outcome::Succeeded, now) else {
            panic!("an applied arrangement must be put back unless kept");
        };
        let late = now + Duration::from_millis(1_300);
        assert_eq!(rollback.seconds_left(late), REVERT_AFTER.as_secs() - 1);
        assert_eq!(rollback.next_tick(late), now + Duration::from_secs(2));
    }
}
