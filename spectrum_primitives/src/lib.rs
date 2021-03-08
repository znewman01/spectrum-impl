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

// These are kind-of leaking. Better to do away with entirely.
pub use constructions::AuthKey;
pub use dpf::multi_key::Key as MultiKeyKey;
pub use dpf::two_key::Key as TwoKeyKey;
pub use prg::ElementVector;
pub use vdpf::multi_key::ProofShare as MultiKeyProof;
pub use vdpf::multi_key::Token as MultiKeyToken;
pub use vdpf::two_key::ProofShare as TwoKeyProof;
pub use vdpf::two_key::Token as TwoKeyToken;

use constructions::{AesPrg, AesSeed};
use prg::GroupPrg;

impl TwoKeyVdpf {
    pub fn with_channels_msg_size(channels: usize, msg_size: usize) -> Self {
        TwoKeyVdpf::new(dpf::TwoKeyDpf::new(AesPrg::new(msg_size), channels)).into()
    }
}

impl MultiKeyVdpf {
    pub fn with_channels_parties_msg_size(channels: usize, groups: usize, msg_size: usize) -> Self {
        use std::convert::TryFrom;
        let seed: AesSeed = AesSeed::try_from(vec![0u8; 16]).unwrap();
        let prg = GroupPrg::from_seed((msg_size - 1) / 31 + 1, seed);
        let dpf = dpf::MultiKeyDpf::new(prg, channels, groups);
        MultiKeyVdpf::new(dpf).into()
    }
}

#[cfg(feature = "testing")]
pub use constructions::IntsModP;
