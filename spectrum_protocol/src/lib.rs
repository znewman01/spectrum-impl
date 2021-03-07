#![feature(type_ascription)]
mod accumulator;

#[macro_use]
mod definition;

mod insecure;
mod secure;
// pub mod multi_key;
// pub mod wrapper;

pub use accumulator::Accumulatable;
pub use definition::Protocol;

pub use insecure::InsecureProtocol;

#[cfg(feature = "proto")]
pub mod proto {
    tonic::include_proto!("spectrum");

    pub use spectrum_primitives::proto::Integer;
}

#[cfg(test)]
mod tests;
