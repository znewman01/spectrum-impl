use crate::config::store::{Key, Store, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default, Clone, Debug)]
pub(in crate::config) struct InMemoryStore {
    map: Arc<Mutex<HashMap<Key, Value>>>,
}

impl InMemoryStore {
    pub(in crate::config) fn new() -> InMemoryStore {
        InMemoryStore {
            map: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Store for InMemoryStore {
    fn get(&self, key: Key) -> Option<Value> {
        let map = self.map.lock().unwrap();
        map.get(&key).cloned()
    }

    fn put(&self, key: Key, value: Value) -> Option<Value> {
        let mut map = self.map.lock().unwrap();
        map.insert(key, value)
    }

    fn list(&self, prefix: Key) -> Vec<(Key, Value)> {
        let map = self.map.lock().unwrap();
        let mut res = Vec::new();
        for (key, value) in map.iter() {
            if key.starts_with(&prefix) {
                res.push((key.clone(), value.clone()));
            }
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_tests;
    use proptest::prelude::*;
    use proptest::strategy::LazyJust;

    pub fn stores() -> BoxedStrategy<impl Store> {
        prop_oneof![LazyJust::new(InMemoryStore::new),].boxed()
    }

    store_tests! {}
}
