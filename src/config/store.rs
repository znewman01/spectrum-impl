// TODO(zjn): disallow empty keys/key components? and key components with "/"
pub type Key = Vec<String>;
pub type Value = String;

pub trait Store: Clone + core::fmt::Debug {
    fn get(&self, key: Key) -> Option<Value>;

    fn put(&self, key: Key, value: Value) -> Option<Value>;

    fn list(&self, prefix: Key) -> Vec<(Key, Value)>;
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
