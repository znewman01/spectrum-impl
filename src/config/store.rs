// TODO(zjn): disallow empty keys/key components? and key components with "/"
pub type Key = Vec<String>;
pub type Value = String;

// TODO(zjn): make these return futures
pub trait Store: Clone + core::fmt::Debug {
    fn get(&self, key: Key) -> Result<Option<Value>, String>;

    fn put(&self, key: Key, value: Value) -> Result<(), String>;

    fn list(&self, prefix: Key) -> Result<Vec<(Key, Value)>, String>;
}

#[cfg(test)]
pub(in crate::config) mod tests {
    use proptest::collection::{vec, VecStrategy};
    use proptest::string::{string_regex, RegexGeneratorStrategy};

    pub fn keys() -> VecStrategy<RegexGeneratorStrategy<String>> {
        vec(string_regex("[[[:word:]]-]+").unwrap(), 0..10usize)
    }

    pub fn values() -> RegexGeneratorStrategy<String> {
        string_regex("\\PC*").unwrap()
    }
}
