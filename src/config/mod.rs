pub mod factory;
mod inmem;
pub mod store;
#[cfg(test)]
pub mod test_macros;

pub use factory::from_env;
pub use store::{Key, Store, Value};

#[cfg(test)]
pub mod tests {
    use super::*;
    pub use inmem::tests::stores as inmem_stores;
    pub use store::tests::{keys, values, KEY};
}
