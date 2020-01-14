#[macro_export]
macro_rules! store_tests {
    () => {
        use $crate::config::store::tests::{keys, values};
        use proptest::collection::hash_set;
        use std::collections::HashSet;

        proptest! {

            #[test]
            fn test_put_and_get(store in stores(), key in keys(), value in values()) {
                store.put(key.clone(), value.clone());
                prop_assert_eq!(store.get(key).unwrap(), value);
            }

            #[test]
            fn test_get_empty(store in stores(), key in keys()) {
                prop_assert!(store.get(key).is_none());
            }

            #[test]
            fn test_put_and_get_keep_latter(store in stores(), key in keys(), value1 in values(), value2 in values()) {
                store.put(key.clone(), value1);
                store.put(key.clone(), value2.clone());
                prop_assert_eq!(store.get(key).unwrap(), value2);
            }

            #[test]
            fn test_list(store in stores(),
                         prefix in keys(),
                         suffixes in hash_set(keys(), 0..10usize),
                         other_keys in hash_set(keys(), 0..10usize),
                         value in values()) {
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
}
