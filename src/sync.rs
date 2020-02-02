use std::ops::{Deref, DerefMut};
use tokio::sync::{oneshot, Mutex};

pub struct OneshotCache<T> {
    cache: Mutex<Option<T>>,
    channel: Mutex<oneshot::Receiver<T>>,
}

impl<T: Clone> OneshotCache<T> {
    pub fn new(rx: oneshot::Receiver<T>) -> Self {
        OneshotCache {
            cache: Mutex::new(None),
            channel: Mutex::new(rx),
        }
    }

    pub async fn get(&self) -> T {
        let mut cache_guard = self.cache.lock().await;
        match cache_guard.deref().as_ref() {
            Some(value) => (*value).clone(),
            None => {
                let value = self.channel.lock().await.deref_mut().await.unwrap();
                *cache_guard = Some(value.clone());
                value
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache() {
        let (tx, rx) = oneshot::channel();
        tx.send(()).expect("TX should succeed.");

        let cache = OneshotCache::new(rx);

        assert_eq!(cache.get().await, ());
        assert_eq!(cache.get().await, ());
    }
}
