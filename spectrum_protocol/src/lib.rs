#![feature(type_ascription)]
mod accumulator;

#[macro_use]
mod definition;

mod insecure;
mod secure;
// pub mod wrapper;

pub use accumulator::Accumulatable;
pub use definition::Protocol;

pub use insecure::InsecureProtocol;

#[cfg(test)]
mod tests;

#[cfg(feature = "proto")]
mod proto {
    include!(concat!(env!("OUT_DIR"), "/spectrum.rs"));
}
