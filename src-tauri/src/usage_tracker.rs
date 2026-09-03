// FR-8: Request Activity & Rate Limit Visibility
// In-memory, session-scoped usage tracker. Zero disk writes — all state resets on app restart.
// Uses DashMap for lock-free concurrent access from async request handlers.

use dashmap::DashMap;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(3600); // 60-minute rolling window

// Per-provider bucket stored in the DashMap
struct ProviderBucket {
    session_requests: AtomicU64,
    timestamps: Mutex<VecDeque<Instant>>,
    rate_limit_remaining: Mutex<Option<u64>>,
}

impl ProviderBucket {
    fn new() -> Self {
        Self {
            session_requests: AtomicU64::new(0),
            timestamps: Mutex::new(VecDeque::new()),
            rate_limit_remaining: Mutex::new(None),
        }
    }

    fn record(&self) {
        self.session_requests.fetch_add(1, Ordering::Relaxed);
        let now = Instant::now();
        let mut ts = self.timestamps.lock().unwrap();
        ts.push_back(now);
        // Prune entries older than 60 minutes
        while ts.front().map_or(false, |t| now.duration_since(*t) > WINDOW) {
            ts.pop_front();
        }
    }

    fn set_rate_limit(&self, val: Option<u64>) {
        *self.rate_limit_remaining.lock().unwrap() = val;
    }

    fn snapshot(&self) -> ProviderUsage {
        let now = Instant::now();
        let mut ts = self.timestamps.lock().unwrap();
        // Prune on read too (handles idle apps that never send requests)
        while ts.front().map_or(false, |t| now.duration_since(*t) > WINDOW) {
            ts.pop_front();
        }
        ProviderUsage {
            requests_this_session: self.session_requests.load(Ordering::Relaxed),
            requests_last_hour: ts.len() as u64,
            rate_limit_remaining: *self.rate_limit_remaining.lock().unwrap(),
        }
    }
}

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct ProviderUsage {
    pub requests_this_session: u64,
    pub requests_last_hour: u64,
    /// None when the provider did not include a rate-limit header on the last response.
    /// Serialized as JSON null — never omitted (FR-8.3).
    pub rate_limit_remaining: Option<u64>,
}

#[derive(Serialize, Clone)]
pub struct UsageSnapshot {
    pub groq: ProviderUsage,
    pub gemini: ProviderUsage,
    pub openrouter: ProviderUsage,
}

// ── Tracker ──────────────────────────────────────────────────────────────────

pub struct UsageTracker {
    buckets: DashMap<String, ProviderBucket>,
}

impl UsageTracker {
    pub fn new() -> Self {
        let buckets: DashMap<String, ProviderBucket> = DashMap::new();
        // Pre-insert all three providers so snapshots always include all keys
        buckets.insert("groq".to_string(), ProviderBucket::new());
        buckets.insert("gemini".to_string(), ProviderBucket::new());
        buckets.insert("openrouter".to_string(), ProviderBucket::new());
        Self { buckets }
    }

    /// Called after every successfully forwarded request.
    pub fn record_request(&self, provider: &str) {
        if let Some(bucket) = self.buckets.get(provider) {
            bucket.record();
        }
    }

    /// Called with whatever the provider returned in its rate-limit headers.
    /// Pass None when the provider included no such header.
    pub fn set_rate_limit_remaining(&self, provider: &str, val: Option<u64>) {
        if let Some(bucket) = self.buckets.get(provider) {
            bucket.set_rate_limit(val);
        }
    }

    /// Returns a point-in-time snapshot for all three providers.
    pub fn get_usage_snapshot(&self) -> UsageSnapshot {
        UsageSnapshot {
            groq: self.buckets.get("groq").map(|b| b.snapshot()).unwrap_or(ProviderUsage {
                requests_this_session: 0,
                requests_last_hour: 0,
                rate_limit_remaining: None,
            }),
            gemini: self.buckets.get("gemini").map(|b| b.snapshot()).unwrap_or(ProviderUsage {
                requests_this_session: 0,
                requests_last_hour: 0,
                rate_limit_remaining: None,
            }),
            openrouter: self.buckets.get("openrouter").map(|b| b.snapshot()).unwrap_or(ProviderUsage {
                requests_this_session: 0,
                requests_last_hour: 0,
                rate_limit_remaining: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_snapshot_is_zeroed() {
        let tracker = UsageTracker::new();
        let snap = tracker.get_usage_snapshot();
        assert_eq!(snap.groq.requests_this_session, 0);
        assert_eq!(snap.groq.requests_last_hour, 0);
        assert!(snap.groq.rate_limit_remaining.is_none());
        assert_eq!(snap.gemini.requests_this_session, 0);
        assert_eq!(snap.gemini.requests_last_hour, 0);
        assert_eq!(snap.openrouter.requests_this_session, 0);
        assert_eq!(snap.openrouter.requests_last_hour, 0);
    }

    #[test]
    fn test_record_and_snapshot() {
        let tracker = UsageTracker::new();
        tracker.record_request("groq");
        tracker.record_request("groq");
        tracker.record_request("gemini");
        let snap = tracker.get_usage_snapshot();
        assert_eq!(snap.groq.requests_this_session, 2);
        assert_eq!(snap.groq.requests_last_hour, 2);
        assert_eq!(snap.gemini.requests_this_session, 1);
        assert_eq!(snap.gemini.requests_last_hour, 1);
        assert_eq!(snap.openrouter.requests_this_session, 0);
        assert_eq!(snap.openrouter.requests_last_hour, 0);
    }

    #[test]
    fn test_rate_limit_set_and_cleared() {
        let tracker = UsageTracker::new();
        tracker.set_rate_limit_remaining("groq", Some(100));
        assert_eq!(tracker.get_usage_snapshot().groq.rate_limit_remaining, Some(100));
        tracker.set_rate_limit_remaining("groq", None);
        assert!(tracker.get_usage_snapshot().groq.rate_limit_remaining.is_none());
    }
}
