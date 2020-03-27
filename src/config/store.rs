use async_trait::async_trait;

// TODO(zjn): change all references to this to be direct to crate::Error
pub use crate::Error;

// TODO(zjn): disallow empty keys/key components? and key components with "/"
pub type Key = Vec<String>;
pub type Value = String;

#[async_trait]
pub trait Store: std::fmt::Debug {
    async fn get(&self, key: Key) -> Result<Option<Value>, Error>;

    async fn put(&self, key: Key, value: Value) -> Result<(), Error>;

    async fn list(&self, prefix: Key) -> Result<Vec<(Key, Value)>, Error>;
}

#[cfg(test)]
pub(in crate::config) mod tests {
    use super::*;
    use proptest::collection::{vec, VecStrategy};
    use proptest::string::{string_regex, RegexGeneratorStrategy};
    use std::collections::HashSet;

    type TestResult = Result<(), Box<dyn std::error::Error + Sync + Send>>;

    pub const KEY: &str = "[[[:word:]]-]+";

    pub fn keys() -> VecStrategy<RegexGeneratorStrategy<String>> {
        vec(string_regex(KEY).unwrap(), 1..10usize)
    }

    pub fn values() -> RegexGeneratorStrategy<String> {
        string_regex("\\PC*").unwrap()
    }

    pub async fn run_test_put_and_get<C: Store>(store: C, key: Key, value: Value) -> TestResult {
        store.put(key.clone(), value.clone()).await?;
        assert_eq!(store.get(key).await?, Some(value));
        Ok(())
    }

    pub async fn run_test_get_empty<C: Store>(store: C, key: Key) -> TestResult {
        assert!(store.get(key).await?.is_none());
        Ok(())
    }

    pub async fn run_test_put_and_get_keep_latter<C: Store>(
        store: C,
        key: Key,
        value1: Value,
        value2: Value,
    ) -> TestResult {
        store.put(key.clone(), value1).await?;
        store.put(key.clone(), value2.clone()).await?;
        assert_eq!(store.get(key).await?, Some(value2));
        Ok(())
    }

    pub async fn run_test_list<C: Store>(
        store: C,
        prefix: Key,
        suffixes: HashSet<Key>,
        other_keys: HashSet<Key>,
        value: Value,
    ) -> TestResult {
        for suffix in &suffixes {
            let key: Key = prefix
                .iter()
                .cloned()
                .chain(suffix.iter().cloned())
                .collect();
            store.put(key, value.clone()).await?;
        }
        for key in other_keys {
            if key.starts_with(&prefix) {
                continue;
            }
            store.put(key, value.clone()).await?;
        }

        let result = store.list(prefix.clone()).await?;

        let actual_keys: HashSet<Key> = result.iter().map(|(k, _v)| k.clone()).collect();
        let expected_keys: HashSet<Key> = suffixes
            .iter()
            .map(|s| prefix.iter().cloned().chain(s.iter().cloned()).collect())
            .collect();
        assert_eq!(actual_keys, expected_keys);

        for (_k, v) in result {
            assert_eq!(&v, &value);
        }

        Ok(())
    }
}
