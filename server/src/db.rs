use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;

#[derive(Clone)]
pub struct Database {
    pub(crate) store: Arc<DashMap<String, Vec<u8>>>,
    pub(crate) expirations: Arc<DashMap<String, Instant>>, // key -> expiry time
}

impl Database {
    pub fn new() -> Self {
        Self {
            store: Arc::new(DashMap::new()),
            expirations: Arc::new(DashMap::new()),
        }
    }

    fn remove_if_expired(&self, key: &str) -> bool {
        if let Some(exp) = self.expirations.get(key) {
            if Instant::now() >= *exp {
                drop(exp);
                self.expirations.remove(key);
                self.store.remove(key);
                return true;
            }
        }
        false
    }

    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        if self.remove_if_expired(key) {
            return None;
        }
        self.store.get(key).map(|v| v.clone())
    }

    pub fn set(&self, key: String, value: Vec<u8>, ttl: Option<Duration>) {
        self.store.insert(key.clone(), value);
        match ttl {
            Some(dur) => {
                self.expirations.insert(key, Instant::now() + dur);
            }
            None => {
                self.expirations.remove(&key);
            }
        }
    }

    pub fn del(&self, keys: &[String]) -> usize {
        let mut deleted = 0usize;
        for key in keys {
            let _ = self.expirations.remove(key);
            if self.store.remove(key).is_some() {
                deleted += 1;
            }
        }
        deleted
    }

    pub fn exists(&self, keys: &[String]) -> usize {
        let mut count = 0usize;
        for key in keys {
            if self.remove_if_expired(key) {
                continue;
            }
            if self.store.contains_key(key) {
                count += 1;
            }
        }
        count
    }

    pub fn incr_by(&self, key: String, delta: i64) -> Result<i64, String> {
        loop {
            if self.remove_if_expired(&key) {}
            match self.store.get(&key) {
                None => {
                    let new_val = delta;
                    self.store
                        .insert(key.clone(), new_val.to_string().into_bytes());
                    return Ok(new_val);
                }
                Some(existing) => {
                    let s = match std::str::from_utf8(&existing) {
                        Ok(s) => s,
                        Err(_) => return Err("value is not an integer or out of range".to_string()),
                    };
                    let curr: i64 = match s.parse() {
                        Ok(i) => i,
                        Err(_) => return Err("value is not an integer or out of range".to_string()),
                    };
                    drop(existing);
                    let new_val = curr.saturating_add(delta);
                    self.store
                        .insert(key.clone(), new_val.to_string().into_bytes());
                    return Ok(new_val);
                }
            }
        }
    }

    pub fn expire_seconds(&self, key: &str, seconds: i64) -> bool {
        if !self.store.contains_key(key) {
            return false;
        }
        if seconds < 0 {
            self.store.remove(key);
            self.expirations.remove(key);
            return true;
        }
        let when = Instant::now() + Duration::from_secs(seconds as u64);
        self.expirations.insert(key.to_string(), when);
        true
    }

    pub fn ttl_seconds(&self, key: &str) -> i64 {
        if self.remove_if_expired(key) {
            return -2;
        }
        if !self.store.contains_key(key) {
            return -2;
        }
        match self.expirations.get(key) {
            None => -1,
            Some(exp) => {
                let now = Instant::now();
                if *exp <= now {
                    drop(exp);
                    self.expirations.remove(key);
                    self.store.remove(key);
                    -2
                } else {
                    let remaining = *exp - now;
                    remaining.as_secs() as i64
                }
            }
        }
    }

    pub fn flushdb(&self) {
        self.store.clear();
        self.expirations.clear();
    }
}

pub async fn start_expiry_reaper(db: Database) {
    let expirations = db.expirations.clone();
    let store = db.store.clone();
    tokio::spawn(async move {
        let interval_ms: u64 = std::env::var("RUSTCACHE_REAPER_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(500);
        let interval = std::time::Duration::from_millis(interval_ms);
        loop {
            tokio::time::sleep(interval).await;
            let now = Instant::now();
            let mut to_remove: Vec<String> = Vec::new();
            for entry in expirations.iter() {
                if *entry.value() <= now {
                    to_remove.push(entry.key().clone());
                }
            }
            for k in to_remove {
                expirations.remove(&k);
                store.remove(&k);
            }
        }
    });
}
