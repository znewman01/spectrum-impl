use crate::{accumulator::Accumulatable, Protocol};

use spectrum_primitives::{Dpf, Vdpf};

use std::fmt;
use std::iter::repeat;

// impl SecureProtocol<BasicVdpf> {
//     pub fn with_aes_prg_dpf(channels: usize, msg_size: usize) -> Self {
//         let vdpf = FieldVdpf::new(BasicDpf::new(AesPrg::new(16, msg_size), channels));
//         SecureProtocol::new(vdpf)
//     }
// }
//
// impl SecureProtocol<MultiKeyVdpf> {
//     pub fn with_group_prg_dpf(channels: usize, groups: usize, msg_size: usize) -> Self {
//         let seed: AesSeed = AesSeed::from(vec![0u8; 16]);
//         let prg: GroupPrg<_> = GroupPrg::from_aes_seed((msg_size - 1) / 31 + 1, seed);
//         let dpf: MultiKeyDpf<GroupPrg<_>> = MultiKeyDpf::new(prg, channels, groups);
//         let vdpf = FieldVdpf::new(dpf);
//         SecureProtocol::new(vdpf)
//     }
// }

impl<V> Protocol for V
where
    V: Vdpf,
    V::Token: Clone,
    <V as Dpf>::Key: fmt::Debug,
    <V as Dpf>::Message: Accumulatable + Clone,
{
    type ChannelKey = <V as Vdpf>::AuthKey;
    type WriteToken = (<V as Dpf>::Key, <V as Vdpf>::ProofShare);
    type AuditShare = <V as Vdpf>::Token;
    type Accumulator = <V as Dpf>::Message;

    fn num_parties(&self) -> usize {
        self.keys()
    }

    fn num_channels(&self) -> usize {
        self.points()
    }

    fn message_len(&self) -> usize {
        self.msg_size()
    }

    fn broadcast(
        &self,
        message: Self::Accumulator,
        idx: usize,
        key: Self::ChannelKey,
    ) -> Vec<Self::WriteToken> {
        let dpf_keys = self.gen(message, idx);
        let proof_shares = self.gen_proofs(&key, idx, &dpf_keys);
        Iterator::zip(dpf_keys.into_iter(), proof_shares.into_iter()).collect()
    }

    fn cover(&self) -> Vec<Self::WriteToken> {
        let dpf_keys = self.gen_empty();
        let proof_shares = self.gen_proofs_noop();
        Iterator::zip(dpf_keys.into_iter(), proof_shares.into_iter()).collect()
    }

    fn gen_audit(
        &self,
        keys: &[Self::ChannelKey],
        token: Self::WriteToken,
    ) -> Vec<Self::AuditShare> {
        let token = self.gen_audit(&keys, &token.0, token.1);
        repeat(token).take(self.num_parties()).collect()
    }

    fn check_audit(&self, tokens: Vec<Self::AuditShare>) -> bool {
        assert_eq!(tokens.len(), self.num_parties());
        self.check_audit(tokens)
    }

    fn new_accumulator(&self) -> Vec<Self::Accumulator> {
        vec![self.null_message(); self.num_channels()]
    }

    fn to_accumulator(&self, token: Self::WriteToken) -> Vec<Self::Accumulator> {
        self.eval(token.0)
    }
}

// #[cfg(test)]
// pub mod tests {
//     use super::*;
//
//     use crate::tests::*;
//     use spectrum_primitives::group::GroupElement;
//
//
//     proptest! {
//         #[test]
//         #[cfg(feature = "proto")]
//         fn test_write_token_proto_roundtrip(token in any::<WriteToken<BasicVdpf>>()) {
//             let wrapped: proto::WriteToken = token.clone().into();
//             assert_eq!(token, wrapped.try_into().unwrap());
//         }
//
//         #[test]
//         #[cfg(feature = "proto")]
//         fn test_audit_share_proto_roundtrip(share in any::<AuditShare<BasicVdpf>>()) {
//             let wrapped: proto::AuditShare = share.clone().into();
//             assert_eq!(share, wrapped.try_into().unwrap());
//         }
//     }
//
//     proptest! {
//         #[test]
//         #[cfg(feature = "proto")]
//         fn test_multi_key_write_token_proto_roundtrip(token in any::<WriteToken<MultiKeyVdpf>>()) {
//             let wrapped: proto::WriteToken = token.clone().into();
//             assert_eq!(token, wrapped.try_into().unwrap());
//         }
//
//         #[test]
//         #[cfg(feature = "proto")]
//         fn test_multi_key_audit_share_proto_roundtrip(share in any::<AuditShare<MultiKeyVdpf>>()) {
//             let wrapped: proto::AuditShare = share.clone().into();
//             assert_eq!(share, wrapped.try_into().unwrap());
//         }
//     }
// }
