#[macro_export]
macro_rules! store_tests {
    () => {
        use futures::executor::block_on;
        use proptest::collection::hash_set;
        use std::collections::HashSet;
        use $crate::config::store::tests::{keys, values};

        // Can't use e.g. tokio::test with proptest
        proptest! {

            #[test]
            fn test_put_and_get(store in stores(), key in keys(), value in values()) {
                let test = async {
                    store.put(key.clone(), value.clone()).await?;
                    prop_assert_eq!(store.get(key).await?, Some(value));
                    Ok(())
                };
                block_on(test)?
            }

            #[test]
            fn test_get_empty(store in stores(), key in keys()) {
                let test = async {
                    prop_assert!(store.get(key).await?.is_none());
                    Ok(())
                };
                block_on(test)?
            }

           #[test]
           fn test_put_and_get_keep_latter(store in stores(), key in keys(), value1 in values(), value2 in values()) {
               let test = async {
                   store.put(key.clone(), value1).await?;
                   store.put(key.clone(), value2.clone()).await?;
                   prop_assert_eq!(store.get(key).await?, Some(value2));
                   Ok(())
               };
               block_on(test)?
           }

           #[test]
           fn test_list(store in stores(),
                        prefix in keys(),
                        suffixes in hash_set(keys(), 0..10usize),
                        other_keys in hash_set(keys(), 0..10usize),
                        value in values()) {
               let test = async {
                   for suffix in &suffixes {
                       let key: Key = prefix.iter().cloned().chain(suffix.iter().cloned()).collect();
                       store.put(key, value.clone()).await?;
                   }
                   for key in other_keys {
                       if key.starts_with(&prefix) {
                           continue;
                       }
                       store.put(key, value.clone()).await?;
                   }

                   let result = store.list(prefix.clone()).await?;

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

                   Ok(())
               };
               block_on(test)?
           }
        }
    };
}
