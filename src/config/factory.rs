use crate::config::inmem::InMemoryStore;
use crate::config::store::Store;
use log::{debug, trace};

static CONFIG_SERVER_ENV_VAR: &str = "SPECTRUM_CONFIG_SERVER";

pub fn from_string(s: &str) -> Result<impl Store, String> {
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
    use proptest::prelude::*;
    use std::any::{Any, TypeId};

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

    #[allow(dead_code)] // TODO(zjn): implement as #[test]
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
