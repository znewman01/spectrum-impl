use crate::config::store::{Error, Key, Store, Value};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default, Clone, Debug)]
pub struct InMemoryStore {
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
    use crate::config::store::tests::*;

    use futures::executor::block_on;
    use proptest::collection::hash_set;
    use proptest::prelude::*;
    use proptest::strategy::LazyJust;

    pub fn stores() -> impl Strategy<Value = InMemoryStore> {
        LazyJust::new(InMemoryStore::new)
    }

    proptest! {
        #[test]
        fn test_put_and_get(store in stores(), key in keys(), value in values()) {
            let test = run_test_put_and_get(store, key, value);
            block_on(test).unwrap()
        }

        #[test]
        fn test_get_empty(store in stores(), key in keys()) {
            let test = run_test_get_empty(store, key);
            block_on(test).unwrap()
        }

        #[test]
        fn test_put_and_get_keep_latter(
            store in stores(),
            key in keys(),
            value1 in values(),
            value2 in values()
        ) {
            let test = run_test_put_and_get_keep_latter(store, key, value1, value2) ;
            block_on(test).unwrap()
        }

        #[test]
        fn test_list(
            store in stores(),
            prefix in keys(),
            suffixes in hash_set(keys(), 0..10usize),
            other_keys in hash_set(keys(), 0..10usize),
            value in values()
        ) {
            let test = run_test_list(store, prefix, suffixes, other_keys, value);
            block_on(test).unwrap()
        }
    }
}
