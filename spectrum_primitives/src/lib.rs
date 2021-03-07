#![feature(type_ascription)]
#![allow(dead_code)] // for now
#[macro_use]
mod algebra;
#[macro_use]
mod util;
#[macro_use]
mod sharing;
#[macro_use]
mod prg;
#[macro_use]
mod bytes;
#[macro_use]
mod dpf;
#[macro_use]
mod vdpf;

mod constructions;

pub use algebra::Group;
pub use bytes::Bytes;
pub use dpf::Dpf;
pub use prg::Prg;
pub use vdpf::Vdpf;

pub use constructions::MultiKeyVdpf;
pub use constructions::TwoKeyVdpf;
pub use prg::ElementVector;

#[cfg(feature = "testing")]
pub use constructions::IntsModP;
