use super::aes_prg::{AESSeed, AESPRG};
use super::jubjub;
use crate::bytes::Bytes;
use crate::vdpf::FieldVDPF;

impl From<AESSeed> for jubjub::Scalar {
    fn from(rhs: AESSeed) -> jubjub::Scalar {
        use std::convert::TryInto;
        let bytes: Bytes = rhs.into();
        bytes.try_into().unwrap()
    }
}

mod two_key_vdpf_with_jubjub {
    use super::*;
    use crate::dpf::TwoKeyDpf;
    check_vdpf!(FieldVDPF<TwoKeyDpf<AESPRG>, jubjub::Scalar>);
}

mod many_key_vdpf_with_jubjub {
    use super::*;
    use crate::dpf::MultiKeyDpf;
    use crate::prg::GroupPRG;
    check_vdpf!(FieldVDPF<MultiKeyDpf<GroupPRG<jubjub::CurvePoint>>, jubjub::Scalar>);
}
