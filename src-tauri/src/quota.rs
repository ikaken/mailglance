//! Client-side pacing for Gmail's per-user quota.
//!
//! Projects created on or after 2026-05-01 get 6,000 units per user per minute, and
//! messages.get costs 20 units (60 for format=full). Calls wait here instead of
//! hitting the limit and failing.

use std::collections::VecDeque;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

/// Stays below Google's 6,000 to leave room for timing differences.
const UNITS_PER_MINUTE: u32 = 5_000;
const WINDOW: Duration = Duration::from_secs(60);

/// Unit costs from https://developers.google.com/workspace/gmail/api/reference/quota
pub mod cost {
    pub const GET_PROFILE: u32 = 1;
    pub const HISTORY_LIST: u32 = 2;
    pub const LABELS_LIST: u32 = 1;
    pub const LABELS_GET: u32 = 1;
    pub const MESSAGES_LIST: u32 = 5;
    pub const MESSAGES_GET: u32 = 20;
    pub const MESSAGES_GET_FULL: u32 = 60;
    pub const MESSAGES_MODIFY: u32 = 5;
    pub const MESSAGES_TRASH: u32 = 20;
    pub const DRAFTS_CREATE: u32 = 10;
    pub const ATTACHMENTS_GET: u32 = 20;
}

#[derive(Default)]
pub struct Quota {
    spent: Mutex<VecDeque<(Instant, u32)>>,
}

fn used(spent: &mut VecDeque<(Instant, u32)>, now: Instant) -> u32 {
    while spent.front().is_some_and(|(t, _)| now.duration_since(*t) >= WINDOW) {
        spent.pop_front();
    }
    spent.iter().map(|(_, c)| c).sum()
}

impl Quota {
    /// Waits until `units` fit in the last minute's budget, then records them.
    pub async fn acquire(&self, units: u32) {
        loop {
            let wait = {
                let mut spent = self.spent.lock().await;
                let now = Instant::now();
                if used(&mut spent, now) + units <= UNITS_PER_MINUTE {
                    spent.push_back((now, units));
                    return;
                }
                // The oldest entry leaves the window first.
                spent.front().map(|(t, _)| WINDOW - now.duration_since(*t)).unwrap_or(WINDOW)
            };
            tokio::time::sleep(wait + Duration::from_millis(50)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn waits_when_budget_is_spent() {
        let q = Quota::default();
        q.acquire(UNITS_PER_MINUTE).await;
        let start = Instant::now();
        q.acquire(10).await;
        assert!(Instant::now().duration_since(start) >= WINDOW);
    }
}
