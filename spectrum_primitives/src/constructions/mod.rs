mod aes_prg;
mod baby;
pub mod jubjub;

use crate::bytes::Bytes;
use crate::dpf::{MultiKeyDpf, TwoKeyDpf};
use crate::prg::GroupPrg;
use crate::vdpf::FieldVdpf;

pub use self::jubjub::Scalar as AuthKey;
pub use aes_prg::AesPrg;
pub use aes_prg::AesSeed;

impl From<AesSeed> for AuthKey {
    fn from(rhs: AesSeed) -> AuthKey {
        use std::convert::TryInto;
        let bytes: Bytes = rhs.into();
        bytes.try_into().unwrap()
    }
}

pub type TwoKeyVdpf = FieldVdpf<TwoKeyDpf<AesPrg>, AuthKey>;
pub type MultiKeyVdpf = FieldVdpf<MultiKeyDpf<GroupPrg<jubjub::CurvePoint>>, AuthKey>;
#[cfg(feature = "testing")]
pub type IntsModP = baby::IntMod<11>;

#[cfg(test)]
mod tests;
