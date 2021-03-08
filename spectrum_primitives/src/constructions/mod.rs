mod aes_prg;
mod baby;
mod jubjub;

use crate::bytes::Bytes;
use crate::dpf::{MultiKeyDpf, TwoKeyDpf};
use crate::prg::GroupPrg;
use crate::vdpf::FieldVdpf;

use aes_prg::AesPrg;

pub use aes_prg::AesSeed;

impl From<AesSeed> for jubjub::Scalar {
    fn from(rhs: AesSeed) -> jubjub::Scalar {
        use std::convert::TryInto;
        let bytes: Bytes = rhs.into();
        bytes.try_into().unwrap()
    }
}

pub type TwoKeyVdpf = FieldVdpf<TwoKeyDpf<AesPrg>, jubjub::Scalar>;
pub type MultiKeyVdpf = FieldVdpf<MultiKeyDpf<GroupPrg<jubjub::CurvePoint>>, jubjub::Scalar>;
#[cfg(feature = "testing")]
pub type IntsModP = baby::IntMod<11>;

#[cfg(test)]
mod tests;
