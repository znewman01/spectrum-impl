use crate::config::store::{Error, Key, Store, Value};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default, Clone, Debug)]
pub(in crate::config) struct InMemoryStore {
    map: Arc<Mutex<HashMap<Key, Value>>>,
}

impl InMemoryStore {
    pub(in crate::config) fn new() -> InMemoryStore {
        InMemoryStore::default()
    }
}

#[async_trait]
impl Store for InMemoryStore {
    async fn get(&self, key: Key) -> Result<Option<Value>, Error> {
        let map = self.map.lock().unwrap();
        Ok(map.get(&key).cloned())
    }

    async fn put(&self, key: Key, value: Value) -> Result<(), Error> {
        let mut map = self.map.lock().unwrap();
        map.insert(key, value);
        Ok(())
    }

    async fn list(&self, prefix: Key) -> Result<Vec<(Key, Value)>, Error> {
        let map = self.map.lock().unwrap();
        let mut res = Vec::new();
        for (key, value) in map.iter() {
            if key.starts_with(&prefix) {
                res.push((key.clone(), value.clone()));
            }
        }
        Ok(res)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::store_tests;
    use proptest::prelude::*;
    use proptest::strategy::LazyJust;

    pub fn stores() -> BoxedStrategy<impl Store> {
        LazyJust::new(InMemoryStore::new).boxed()
    }

    store_tests! {}
}
