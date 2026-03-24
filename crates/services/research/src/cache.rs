use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::ResearchResult;

const DEFAULT_TTL: Duration = Duration::from_secs(24 * 60 * 60);

pub struct ResearchCache {
    entries: HashMap<String, CacheEntry>,
    ttl: Duration,
}

struct CacheEntry {
    result: ResearchResult,
    inserted_at: Instant,
}

impl ResearchCache {
    pub fn new() -> Self {
        Self::with_ttl(DEFAULT_TTL)
    }

    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            ttl,
        }
    }

    pub fn get(&self, key: &str) -> Option<ResearchResult> {
        self.entries.get(key).and_then(|entry| {
            if entry.inserted_at.elapsed() < self.ttl {
                Some(entry.result.clone())
            } else {
                None
            }
        })
    }

    pub fn insert(&mut self, key: String, result: ResearchResult) {
        self.entries.insert(
            key,
            CacheEntry {
                result,
                inserted_at: Instant::now(),
            },
        );
    }

    pub fn prune_expired(&mut self) {
        self.entries
            .retain(|_, entry| entry.inserted_at.elapsed() < self.ttl);
    }
}
