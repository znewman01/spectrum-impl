use crate::{
    config::{
        inmem::InMemoryStore,
        store::{Key, Store, Value},
    },
    Error,
};
use log::{debug, trace};

static CONFIG_SERVER_ENV_VAR: &str = "SPECTRUM_CONFIG_SERVER";

#[derive(Clone, Debug)]
pub enum Wrapper {
    InMem(InMemoryStore),
}

impl From<InMemoryStore> for Wrapper {
    fn from(store: InMemoryStore) -> Self {
        Self::InMem(store)
    }
}

#[tonic::async_trait]
impl Store for Wrapper {
    async fn get(&self, key: Key) -> Result<Option<Value>, Error> {
        match self {
            Wrapper::InMem(store) => store.get(key).await,
        }
    }

    async fn put(&self, key: Key, value: Value) -> Result<(), Error> {
        match self {
            Wrapper::InMem(store) => store.put(key, value).await,
        }
    }

    async fn list(&self, prefix: Key) -> Result<Vec<(Key, Value)>, Error> {
        match self {
            Wrapper::InMem(store) => store.list(prefix).await,
        }
    }
}

pub async fn from_string(s: &str) -> Result<Wrapper, String> {
    let mut scheme = "mem";
    let remainder = if !s.is_empty() {
        let mut chunks = s.splitn(2, "://");
        scheme = chunks.next().expect("");
        chunks.next().ok_or_else(|| {
            format!(
                "Missing scheme separator [://] in config specification [{}]",
                s
            )
        })?
    } else {
        ""
    };

    match scheme {
        "mem" => {
            if remainder.is_empty() {
                debug!("Using in-memory config store.");
                Ok(InMemoryStore::new().into())
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

pub async fn from_env() -> Result<Wrapper, String> {
    let env_str = std::env::var_os(CONFIG_SERVER_ENV_VAR)
        .and_then(|s| s.into_string().ok())
        .unwrap_or_default();
    trace!(
        "Got configuration URL specifier [{}] (from ${}).",
        env_str,
        CONFIG_SERVER_ENV_VAR
    );
    from_string(&env_str).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tokio::runtime::Runtime;

    proptest! {
        #[test]
        #[allow(unused_must_use)]
        fn test_from_string_does_not_crash(string in "\\PC*") {
            from_string(&string);
        }
    }

    #[tokio::test]
    #[allow(irrefutable_let_patterns)]
    async fn test_from_string_empty() {
        let store = from_string("")
            .await
            .expect("Should be Ok() for empty string.");
        if let Wrapper::InMem(_) = store {
        } else {
            panic!("Expected in-memory store");
        }
    }

    #[tokio::test]
    #[allow(irrefutable_let_patterns)]
    async fn test_from_string_mem() {
        let store = from_string("mem://")
            .await
            .expect("Should be Ok() for string [mem://].");
        if let Wrapper::InMem(_) = store {
        } else {
            panic!("Expected in-memory store");
        }
    }

    #[allow(dead_code)] // TODO(zjn): implement as #[test]
    async fn test_from_string_etcd() {
        from_string("etcd://").await.expect("etcd:// should work");
    }

    proptest! {
        #[test]
        fn test_from_string_mem_nonempty(string in "\\PC+") {
            let test = async {
                from_string(&("mem://".to_owned() + &string))
                    .await
                    .expect_err("Non-empty mem:// should error.");
            };
            Runtime::new().unwrap().block_on(test);
        }

        #[test]
        fn test_from_string_other(string in "\\PC+(://)?\\PC*") {
            prop_assume!(string != "mem://");
            let test = async {
                from_string(&string)
                    .await
                    .expect_err("Should only accept mem:// or etcd:// URLs if non-empty.");
            };
            Runtime::new().unwrap().block_on(test);
        }
    }
}
