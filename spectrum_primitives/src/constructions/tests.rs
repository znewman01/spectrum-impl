use super::aes_prg::{AesPrg, AesSeed};
use super::jubjub;
use crate::bytes::Bytes;
use crate::vdpf::FieldVdpf;

impl From<AesSeed> for jubjub::Scalar {
    fn from(rhs: AesSeed) -> jubjub::Scalar {
        use std::convert::TryInto;
        let bytes: Bytes = rhs.into();
        bytes.try_into().unwrap()
    }
}

mod two_key_vdpf_with_jubjub {
    use super::*;
    use crate::dpf::TwoKeyDpf;
    check_vdpf!(FieldVdpf<TwoKeyDpf<AesPrg>, jubjub::Scalar>);
}

mod many_key_vdpf_with_jubjub {
    use super::*;
    use crate::dpf::MultiKeyDpf;
    use crate::prg::GroupPrg;
    check_vdpf!(FieldVdpf<MultiKeyDpf<GroupPrg<jubjub::CurvePoint>>, jubjub::Scalar>);
}
