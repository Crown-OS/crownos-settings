//! The arrangement the worker puts back when nobody keeps the one just applied.

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

    pub fn deadline(&self) -> Instant {
        self.deadline
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
    fn an_applied_arrangement_goes_back_after_the_window() {
        let now = Instant::now();
        let rollback = Rollback::after_apply(Vec::new(), &Outcome::Succeeded, now);
        assert_eq!(
            rollback.map(|rollback| rollback.deadline()),
            Some(now + REVERT_AFTER)
        );
    }
}
