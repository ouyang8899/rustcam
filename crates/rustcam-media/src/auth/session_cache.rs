use std::time::{Duration, Instant};

use dashmap::DashMap;
use uuid::Uuid;

pub struct SessionCache {
    ttl: Duration,
    entries: DashMap<Uuid, Instant>,
}

impl SessionCache {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            ttl: Duration::from_secs(ttl_seconds),
            entries: DashMap::new(),
        }
    }

    pub fn is_fresh(&self, id: Uuid) -> bool {
        match self.entries.get(&id) {
            Some(at) => at.elapsed() < self.ttl,
            None => false,
        }
    }

    pub fn mark(&self, id: Uuid) {
        self.entries.insert(id, Instant::now());
    }

    pub fn remove(&self, id: Uuid) {
        self.entries.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_and_reads_within_ttl() {
        let cache = SessionCache::new(60);
        let id = Uuid::new_v4();
        assert!(!cache.is_fresh(id));
        cache.mark(id);
        assert!(cache.is_fresh(id));
        cache.remove(id);
        assert!(!cache.is_fresh(id));
    }

    #[test]
    fn zero_ttl_is_never_fresh() {
        let cache = SessionCache::new(0);
        let id = Uuid::new_v4();
        cache.mark(id);
        assert!(!cache.is_fresh(id));
    }
}
