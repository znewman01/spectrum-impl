#![feature(type_ascription)]
mod accumulator;

#[macro_use]
mod definition;

pub mod insecure;
// pub mod multi_key;
// pub mod secure;
// pub mod wrapper;

pub use accumulator::Accumulatable;
pub use definition::Protocol;

#[cfg(feature = "proto")]
pub mod proto {
    tonic::include_proto!("spectrum");

    pub use spectrum_primitives::proto::Integer;
}
