//! Waiting for a moment that may never come.

use std::future::pending;
use std::time::Instant;

/// Resolves at `deadline`, or never when there is none.
pub async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
        None => pending().await,
    }
}
