use crate::config::store::{Key, Store, Value};
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

impl Store for InMemoryStore {
    fn get(&self, key: Key) -> Result<Option<Value>, String> {
        let map = self.map.lock().unwrap();
        Ok(map.get(&key).cloned())
    }

    fn put(&self, key: Key, value: Value) -> Result<(), String> {
        let mut map = self.map.lock().unwrap();
        map.insert(key, value);
        Ok(())
    }

    fn list(&self, prefix: Key) -> Result<Vec<(Key, Value)>, String> {
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
mod tests {
    use super::*;
    use crate::store_tests;
    use proptest::prelude::*;
    use proptest::strategy::LazyJust;

    fn stores() -> BoxedStrategy<impl Store> {
        LazyJust::new(InMemoryStore::new).boxed()
    }

    store_tests! {}
}
