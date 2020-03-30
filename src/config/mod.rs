mod etcd;
pub mod factory;
mod inmem;
pub mod store;

pub use factory::{from_env, from_string};
pub use store::{Key, Store, Value};

#[cfg(test)]
pub mod tests {
    use super::*;
    pub use inmem::tests::stores as inmem_stores;
    pub use store::tests::{keys, values, KEY};
}
