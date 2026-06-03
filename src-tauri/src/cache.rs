use std::sync::Mutex;
use std::time::{Instant, Duration};
use std::collections::HashMap;
use crate::api_logic::UnifiedAlert;

const CACHE_DURATION: Duration = Duration::from_secs(300); // 5 minutes

pub struct AlertCache {
    pub alerts: Vec<UnifiedAlert>,
    pub timestamp: Instant,
}

pub struct CacheState {
    pub cache: Mutex<Option<AlertCache>>,
    /// Per-source cache keyed by provider ID string (e.g. "pge", "tauron")
    pub source_cache: Mutex<HashMap<String, AlertCache>>,
}

impl CacheState {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(None),
            source_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self) -> Option<Vec<UnifiedAlert>> {
        let lock = self.cache.lock().unwrap();
        if let Some(c) = lock.as_ref() {
            if c.timestamp.elapsed() < CACHE_DURATION {
                return Some(c.alerts.clone());
            }
        }
        None
    }

    pub fn get_stale(&self) -> Option<Vec<UnifiedAlert>> {
        let lock = self.cache.lock().unwrap();
        lock.as_ref().map(|c| c.alerts.clone())
    }

    pub fn set(&self, alerts: Vec<UnifiedAlert>) {
        let mut lock = self.cache.lock().unwrap();
        *lock = Some(AlertCache {
            alerts,
            timestamp: Instant::now(),
        });
    }

    pub fn clear(&self) {
        let mut lock = self.cache.lock().unwrap();
        *lock = None;
        let mut source_lock = self.source_cache.lock().unwrap();
        source_lock.clear();
    }

    /// Returns cached alerts for a specific source if still within TTL.
    pub fn get_source(&self, source_id: &str) -> Option<Vec<UnifiedAlert>> {
        let lock = self.source_cache.lock().unwrap();
        if let Some(c) = lock.get(source_id) {
            if c.timestamp.elapsed() < CACHE_DURATION {
                return Some(c.alerts.clone());
            }
        }
        None
    }

    /// Stores alerts for a specific source in the per-source cache.
    pub fn set_source(&self, source_id: &str, alerts: Vec<UnifiedAlert>) {
        let mut lock = self.source_cache.lock().unwrap();
        lock.insert(source_id.to_string(), AlertCache {
            alerts,
            timestamp: Instant::now(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_logic::{UnifiedAlert, AlertSource};

    #[test]
    fn test_cache_set_get() {
        let state = CacheState::new();
        let alert = UnifiedAlert {
            source: AlertSource::Tauron,
            message: Some("Test".to_string()),
            ..Default::default()
        };
        state.set(vec![alert.clone()]);

        let cached = state.get().unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].message, Some("Test".to_string()));
    }

    #[test]
    fn test_cache_clear() {
        let state = CacheState::new();
        state.set(vec![]);
        state.clear();
        assert!(state.get().is_none());
    }

    #[test]
    fn test_cache_expiration() {
        let state = CacheState::new();
        let old_time = Instant::now() - Duration::from_secs(400); // Beyond 300s
        {
            let mut lock = state.cache.lock().unwrap();
            *lock = Some(AlertCache {
                alerts: vec![],
                timestamp: old_time,
            });
        }
        assert!(state.get().is_none());
    }

    #[test]
    fn test_source_cache_hit() {
        let state = CacheState::new();
        let alert = UnifiedAlert {
            source: AlertSource::Pge,
            message: Some("PGE outage".to_string()),
            ..Default::default()
        };
        state.set_source("pge", vec![alert]);
        let cached = state.get_source("pge").unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].message, Some("PGE outage".to_string()));
    }

    #[test]
    fn test_source_cache_miss_on_expiry() {
        let state = CacheState::new();
        {
            let mut lock = state.source_cache.lock().unwrap();
            lock.insert("pge".to_string(), AlertCache {
                alerts: vec![],
                timestamp: Instant::now() - Duration::from_secs(400),
            });
        }
        assert!(state.get_source("pge").is_none());
    }
}
