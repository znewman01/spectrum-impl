use log::{debug, trace};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

static CONFIG_SERVER_ENV_VAR: &'static str = "SPECTRUM_CONFIG_SERVER";

// TODO(zjn): disallow empty keys/key components? and key components with "/"
type Key = Vec<String>;
type Value = String;

pub trait Store: Clone + core::fmt::Debug {
    fn get(&self, key: Key) -> Option<Value>;

    fn put(&self, key: Key, value: Value) -> Option<Value>;

    fn list(&self, prefix: Key) -> Vec<(Key, Value)>;
}

#[derive(Default, Clone, Debug)]
struct InMemoryStore {
    map: Arc<Mutex<HashMap<Key, Value>>>,
}

impl InMemoryStore {
    fn new() -> InMemoryStore {
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

pub fn from_string(s: &str) -> Result<impl Store, String> {
    let mut scheme = "mem";
    let mut remainder = "";
    if !s.is_empty() {
        let mut chunks = s.splitn(2, "://");
        scheme = chunks.next().expect("");
        remainder = chunks.next().ok_or(format!(
            "Missing scheme separator [://] in config specification [{}]",
            s
        ))?;
    }

    match scheme {
        "mem" => {
            if remainder.is_empty() {
                debug!("Using in-memory config store.");
                Ok(InMemoryStore::new())
            } else {
                Err(format!(
                    "Expected empty authority for mem:// URL; got [{}].",
                    remainder
                ))
            }
        }
        "etcd" => Err("etcd scheme currently unimplemented".to_string()),
        _ => Err(format!(
            "Unrecognized config server specification [{}]. \
             Expected [mem://] or [etcd://].",
            s
        )),
    }
}

pub fn from_env() -> Result<impl Store, String> {
    let env_str = std::env::var_os(CONFIG_SERVER_ENV_VAR)
        .and_then(|s| s.into_string().ok())
        .unwrap_or_default();
    trace!(
        "Got configuration URL specifier [{}] (from ${}).",
        env_str,
        CONFIG_SERVER_ENV_VAR
    );
    from_string(&env_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::collection::{hash_set, vec, VecStrategy};
    use proptest::prelude::*;
    use proptest::string::{string_regex, RegexGeneratorStrategy};
    use std::any::{Any, TypeId};
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
            let store = InMemoryStore::new();
            store.put(key.clone(), value.clone());
            prop_assert_eq!(store.get(key).unwrap(), value);
        }

        #[test]
        fn test_get_empty(key in key_strat()) {
            let store = InMemoryStore::new();
            prop_assert!(store.get(key).is_none());
        }

        #[test]
        fn test_put_and_get_keep_latter(key in key_strat(), value1 in value_strat(), value2 in value_strat()) {
            let store = InMemoryStore::new();
            store.put(key.clone(), value1);
            store.put(key.clone(), value2.clone());
            prop_assert_eq!(store.get(key).unwrap(), value2);
        }

        #[test]
        fn test_list(prefix in key_strat(),
                     suffixes in hash_set(key_strat(), 0..10usize),
                     other_keys in hash_set(key_strat(), 0..10usize),
                     value in value_strat()) {
            let store = InMemoryStore::new();
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

    proptest! {
        #[test]
        #[allow(unused_must_use)]
        fn test_from_string_does_not_crash(string in "\\PC*") {
            from_string(&string);
        }
    }

    #[test]
    fn test_from_string_empty() {
        let store = from_string("").expect("Should be Ok() for empty string.");
        assert_eq!(
            TypeId::of::<InMemoryStore>(),
            store.type_id(),
            "Expected InMemoryStore."
        );
    }

    #[test]
    fn test_from_string_mem() {
        let store = from_string("mem://").expect("Should be Ok() for string [mem://].");
        assert_eq!(
            TypeId::of::<InMemoryStore>(),
            store.type_id(),
            "Expected InMemoryStore."
        );
    }

    #[allow(dead_code)]  // TODO(zjn): implement as #[test]
    fn test_from_string_etcd() {
        from_string("etcd://").expect("etcd:// should work");
    }

    proptest! {
        #[test]
        fn test_from_string_mem_nonempty(string in "\\PC+") {
            from_string(&("mem://".to_owned() + &string)).expect_err("Non-empty mem:// should error.");
        }

        // strictly speaking *could* give mem:// but unlikely
        #[test]
        fn test_from_string_other(string in "\\PC+(://)?\\PC*") {
            from_string(&string).expect_err("Should only accept mem:// or etcd:// URLs if non-empty.");
        }
    }
}
