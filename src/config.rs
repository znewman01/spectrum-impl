#![allow(dead_code)] // just until we start using this module
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// TODO(zjn): disallow empty keys/key components? and key components with "/"
type Key = Vec<String>;
type Value = String;

pub trait ConfigStore {
    fn get(&self, key: Key) -> Option<Value>;

    fn put(&self, key: Key, value: Value) -> Option<Value>;

    fn list(&self, prefix: Key) -> Vec<(Key, Value)>;
}

#[derive(Default)]
pub struct InMemoryConfigStore {
    map: Arc<Mutex<HashMap<Key, Value>>>,
}

impl InMemoryConfigStore {
    pub fn new() -> InMemoryConfigStore {
        InMemoryConfigStore {
            map: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl ConfigStore for InMemoryConfigStore {
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
    use proptest::collection::{hash_set, vec, VecStrategy};
    use proptest::prelude::*;
    use proptest::string::{string_regex, RegexGeneratorStrategy};
    use std::collections::HashSet;

    fn key_strat() -> VecStrategy<RegexGeneratorStrategy<String>> {
        vec(string_regex("[[[:word:]]-]+").unwrap(), 0..10usize)
    }

    fn value_strat() -> RegexGeneratorStrategy<String> {
        string_regex("\\PC*").unwrap()
    }

    proptest! {
        #[test]
        fn test_put_and_get(key in key_strat(), value in value_strat()) {
            let store = InMemoryConfigStore::new();
            store.put(key.clone(), value.clone());
            prop_assert_eq!(store.get(key).unwrap(), value);
        }

        #[test]
        fn test_get_empty(key in key_strat()) {
            let store = InMemoryConfigStore::new();
            prop_assert!(store.get(key).is_none());
        }

        #[test]
        fn test_put_and_get_keep_latter(key in key_strat(), value1 in value_strat(), value2 in value_strat()) {
            let store = InMemoryConfigStore::new();
            store.put(key.clone(), value1);
            store.put(key.clone(), value2.clone());
            prop_assert_eq!(store.get(key).unwrap(), value2);
        }

        #[test]
        fn test_list(prefix in key_strat(),
                     suffixes in hash_set(key_strat(), 0..10usize),
                     other_keys in hash_set(key_strat(), 0..10usize),
                     value in value_strat()) {
            let store = InMemoryConfigStore::new();
            for suffix in &suffixes {
                let key: Key = prefix.iter().cloned().chain(suffix.iter().cloned()).collect();
                store.put(key, value.clone());
            }
            for key in other_keys {
                if key.starts_with(&prefix) {
                    continue;
                }
                store.put(key, value.clone());
            }

            let result = store.list(prefix.clone());

            let actual_keys: HashSet<Key> = result
                .iter()
                .map(|(k, _v)| { k.clone() })
                .collect();
            let expected_keys: HashSet<Key> = suffixes
                .iter()
                .map(|s| { prefix.iter().cloned().chain(s.iter().cloned()).collect() })
                .collect();
            prop_assert_eq!(actual_keys, expected_keys);

            for (_k, v) in result {
                prop_assert_eq!(&v, &value);
            }
        }
    }
}
