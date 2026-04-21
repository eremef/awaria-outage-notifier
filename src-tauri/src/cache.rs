use std::sync::Mutex;
use std::time::{Instant, Duration};
use crate::api_logic::UnifiedAlert;

const CACHE_DURATION: Duration = Duration::from_secs(300); // 5 minutes

pub struct AlertCache {
    pub alerts: Vec<UnifiedAlert>,
    pub timestamp: Instant,
}

pub struct CacheState {
    pub cache: Mutex<Option<AlertCache>>,
}

impl CacheState {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(None),
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
}
