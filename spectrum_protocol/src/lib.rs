#![feature(type_ascription)]
mod accumulator;

#[macro_use]
mod definition;

pub mod secure;
pub mod wrapper;

pub use accumulator::Accumulatable;
pub use definition::Protocol;

#[cfg(test)]
mod tests;

#[cfg(feature = "proto")]
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/spectrum_protocol.rs"));
}
