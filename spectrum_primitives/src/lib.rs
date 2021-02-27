#![allow(dead_code)] // for now
#[macro_use]
pub mod algebra;
#[macro_use]
pub mod util;
#[macro_use]
mod lss;
#[macro_use]
mod prg;

#[macro_use]
pub mod bytes;
#[macro_use]
pub mod dpf;
// pub mod field;
// pub mod group;
// pub mod vdpf;

mod constructions;

#[cfg(feature = "proto")]
pub mod proto {
    tonic::include_proto!("spectrum_primitives");
}
