use async_trait::async_trait;
use core::fmt;

// TODO(zjn): disallow empty keys/key components? and key components with "/"
pub type Key = Vec<String>;
pub type Value = String;

#[derive(fmt::Debug)]
pub struct Error {
    message: String,
}

impl Error {
    pub fn new(message: &str) -> Error {
        Error {
            message: message.to_string(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<String> for Error {
    fn from(error: String) -> Self {
        Error::new(&error)
    }
}

impl std::error::Error for Error {}

#[async_trait]
pub trait Store: Clone + Sync + Send + fmt::Debug {
    async fn get(&self, key: Key) -> Result<Option<Value>, Error>;

    async fn put(&self, key: Key, value: Value) -> Result<(), Error>;

    async fn list(&self, prefix: Key) -> Result<Vec<(Key, Value)>, Error>;
}

#[cfg(test)]
pub(in crate::config) mod tests {
    use proptest::collection::{vec, VecStrategy};
    use proptest::string::{string_regex, RegexGeneratorStrategy};

    pub const KEY: &str = "[[[:word:]]-]+";

    pub fn keys() -> VecStrategy<RegexGeneratorStrategy<String>> {
        vec(string_regex(KEY).unwrap(), 0..10usize)
    }

    pub fn values() -> RegexGeneratorStrategy<String> {
        string_regex("\\PC*").unwrap()
    }
}
