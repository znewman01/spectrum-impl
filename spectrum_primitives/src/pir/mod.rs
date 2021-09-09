#[macro_use]
mod definition;

mod insecure;
mod linear;

pub use definition::Database;
pub use linear::LinearDatabase;
