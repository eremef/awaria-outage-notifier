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
