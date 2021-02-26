#![feature(iterator_fold_self, min_const_generics)]
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
mod constructions;
// pub mod dpf;
// pub mod field;
// pub mod group;
// pub mod vdpf;

#[cfg(feature = "proto")]
pub mod proto {
    tonic::include_proto!("spectrum_primitives");
}
