#![allow(clippy::unknown_clippy_lints)] // below issue triggers only on clippy beta/nightly
#![allow(clippy::match_single_binding)] // https://github.com/mcarton/rust-derivative/issues/58
use crate::{accumulator::Accumulatable, Protocol};

use derivative::Derivative;
use rug::Integer;
use serde::{Deserialize, Serialize};
use spectrum_primitives::bytes::Bytes;
use spectrum_primitives::{
    dpf::{BasicDPF, MultiKeyDPF, DPF},
    field::Field,
    prg::{
        aes::{AESSeed, AESPRG},
        group::GroupPRG,
    },
    vdpf::{FieldVDPF, VDPF},
};

use std::fmt;
use std::iter::repeat;

#[cfg(test)]
use proptest::prelude::*;

#[cfg(feature = "proto")]
use {
    crate::proto,
    spectrum_primitives::{
        dpf,
        lss::SecretShare,
        prg::PRG,
        vdpf::{self, FieldProofShare, FieldToken},
    },
    std::{
        convert::{TryFrom, TryInto},
        sync::Arc,
    },
};

pub use spectrum_primitives::vdpf::{BasicVdpf, MultiKeyVdpf};

#[derive(Derivative)]
#[derivative(
    Clone(bound = "V::AuthKey: Clone"),
    Debug(bound = "V::AuthKey: fmt::Debug"),
    PartialEq(bound = "V::AuthKey: PartialEq"),
    Eq(bound = "V::AuthKey: Eq")
)]
pub struct ChannelKey<V: VDPF> {
    pub(in crate) idx: usize,
    pub(in crate) secret: V::AuthKey,
}

impl<V: VDPF> ChannelKey<V> {
    pub fn new(idx: usize, secret: V::AuthKey) -> Self {
        ChannelKey { idx, secret }
    }
}

#[derive(Derivative)]
#[derivative(
    Debug(bound = "V::ProofShare: fmt::Debug, <V as DPF>::Key: fmt::Debug"),
    PartialEq(bound = "V::ProofShare: PartialEq, <V as DPF>::Key: PartialEq"),
    Eq(bound = "V::ProofShare: Eq, <V as DPF>::Key: Eq"),
    Clone(bound = "V::ProofShare: Clone, <V as DPF>::Key: Clone")
)]
pub struct WriteToken<V: VDPF>(<V as DPF>::Key, V::ProofShare);

impl<V: VDPF> WriteToken<V> {
    fn new(key: <V as DPF>::Key, proof_share: V::ProofShare) -> Self {
        WriteToken(key, proof_share)
    }
}

#[cfg(feature = "proto")]
impl<D, P> Into<proto::WriteToken> for WriteToken<FieldVDPF<D>>
where
    FieldVDPF<D>: VDPF<Key = dpf::Key<P>, ProofShare = vdpf::FieldProofShare>,
    P: PRG,
    P::Seed: Into<Vec<u8>>,
    P::Output: Into<Vec<u8>>,
{
    fn into(self) -> proto::WriteToken {
        let dpf_key_proto = proto::secure_write_token::DpfKey {
            encoded_msg: self.0.encoded_msg.into(),
            bits: self.0.bits,
            seeds: self
                .0
                .seeds
                .into_iter()
                .map(Into::<Vec<u8>>::into)
                .collect(),
        };
        let is_first = self.1.bit.is_first();
        assert_eq!(is_first, self.1.seed.is_first());
        let bit = self.1.bit.value();
        let modulus: proto::Integer = bit.field().into();
        let proof = proto::secure_write_token::ProofShare {
            bit: Some(bit.into()),
            seed: Some(self.1.seed.value().into()),
            is_first,
        };
        let inner = proto::SecureWriteToken {
            key: Some(dpf_key_proto),
            proof: Some(proof),
            modulus: Some(modulus),
        };
        proto::WriteToken {
            inner: Some(proto::write_token::Inner::Secure(inner)),
        }
    }
}

#[cfg(feature = "proto")]
impl<D, P> TryFrom<proto::WriteToken> for WriteToken<FieldVDPF<D>>
where
    FieldVDPF<D>: VDPF<Key = dpf::Key<P>, ProofShare = vdpf::FieldProofShare>,
    P: PRG,
    P::Seed: From<Vec<u8>>,
    P::Output: TryFrom<Vec<u8>>,
    <P::Output as TryFrom<Vec<u8>>>::Error: std::fmt::Debug,
{
    type Error = &'static str;

    fn try_from(token: proto::WriteToken) -> Result<Self, Self::Error> {
        if let proto::write_token::Inner::Secure(inner) = token.inner.unwrap() {
            let key_proto = inner.key.unwrap();
            let dpf_key = <FieldVDPF<D> as DPF>::Key::new(
                key_proto.encoded_msg.try_into().unwrap(),
                key_proto.bits,
                key_proto.seeds.into_iter().map(Into::into).collect(),
            );
            let modulus: proto::Integer = inner.modulus.unwrap();
            let field = Arc::new(Field::from(modulus));
            let proof_proto = inner.proof.unwrap();
            let proof_share = FieldProofShare::new(
                SecretShare::new(
                    field.from_proto(proof_proto.bit.unwrap()),
                    proof_proto.is_first,
                ),
                SecretShare::new(
                    field.from_proto(proof_proto.seed.unwrap()),
                    proof_proto.is_first,
                ),
            );
            Ok(WriteToken::<FieldVDPF<D>>(dpf_key, proof_share))
        } else {
            Err("Invalid proto::WriteToken.")
        }
    }
}

#[cfg(test)]
impl<V> Arbitrary for WriteToken<V>
where
    V: VDPF,
    <V as DPF>::Key: Arbitrary + 'static,
    V::ProofShare: Arbitrary + 'static,
{
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        (any::<<V as DPF>::Key>(), any::<V::ProofShare>())
            .prop_map(|(dpf_key, share)| WriteToken::new(dpf_key, share))
            .boxed()
    }
}

#[derive(Derivative)]
#[derivative(
    Clone(bound = "V::Token: Clone"),
    Debug(bound = "V::Token: fmt::Debug"),
    PartialEq(bound = "V::Token: PartialEq"),
    Eq(bound = "V::Token: Eq")
)]
pub struct AuditShare<V: VDPF> {
    token: V::Token,
}

impl<V: VDPF> AuditShare<V> {
    fn new(token: V::Token) -> Self {
        AuditShare::<V> { token }
    }
}

#[cfg(feature = "proto")]
impl<V> Into<proto::AuditShare> for AuditShare<V>
where
    V: VDPF<Token = vdpf::FieldToken>,
{
    fn into(self) -> proto::AuditShare {
        let bit = self.token.bit.value();
        let field = bit.field();
        let modulus: proto::Integer = field.into();
        let is_first = self.token.bit.is_first();
        assert_eq!(is_first, self.token.seed.is_first());

        let inner = proto::audit_share::Inner::Secure(proto::SecureAuditShare {
            bit: Some(bit.into()),
            seed: Some(self.token.seed.value().into()),
            is_first,
            data: self.token.data.into(),
            modulus: Some(modulus),
        });
        proto::AuditShare { inner: Some(inner) }
    }
}

#[cfg(feature = "proto")]
impl<V> TryFrom<proto::AuditShare> for AuditShare<V>
where
    V: VDPF<Token = vdpf::FieldToken>,
{
    type Error = &'static str;

    fn try_from(share: proto::AuditShare) -> Result<Self, Self::Error> {
        if let proto::audit_share::Inner::Secure(inner) = share.inner.unwrap() {
            let modulus: proto::Integer = inner.modulus.unwrap();
            let field = Arc::new(Field::from(modulus));

            let bit = field.from_proto(inner.bit.unwrap());
            let seed = field.from_proto(inner.seed.unwrap());

            Ok(Self::new(FieldToken::new(
                SecretShare::new(bit, inner.is_first),
                SecretShare::new(seed, inner.is_first),
                inner.data.into(),
            )))
        } else {
            Err("Invalid proto::AuditShare.")
        }
    }
}

#[cfg(test)]
impl<V> Arbitrary for AuditShare<V>
where
    V: VDPF + 'static,
    V::Token: Arbitrary + fmt::Debug + 'static,
{
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        any::<V::Token>().prop_map(AuditShare::new).boxed()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SecureProtocol<V> {
    pub vdpf: V, // TODO: should remove pub (need to add some method like sample_keys below)
}

impl<V: VDPF> SecureProtocol<V> {
    pub fn new(vdpf: V) -> SecureProtocol<V> {
        SecureProtocol { vdpf }
    }

    #[allow(dead_code)]
    fn sample_keys(&self) -> Vec<ChannelKey<V>> {
        self.vdpf
            .new_access_keys()
            .into_iter()
            .enumerate()
            .map(|(idx, secret)| ChannelKey::<V>::new(idx, secret))
            .collect()
    }
}

fn field_with_security(sec_bits: u32) -> Field {
    Field::from(Integer::from(
        (Integer::from(2) << sec_bits).next_prime_ref(),
    ))
}

impl SecureProtocol<BasicVdpf> {
    pub fn with_aes_prg_dpf(sec_bits: u32, channels: usize, msg_size: usize) -> Self {
        let field = field_with_security(sec_bits);
        let vdpf = FieldVDPF::new(BasicDPF::new(AESPRG::new(16, msg_size), channels), field);
        SecureProtocol::new(vdpf)
    }
}

impl SecureProtocol<MultiKeyVdpf> {
    pub fn with_group_prg_dpf(
        sec_bits: u32,
        channels: usize,
        groups: usize,
        msg_size: usize,
    ) -> Self {
        let seed: AESSeed = AESSeed::from(vec![0u8; 16]);
        let prg: GroupPRG = GroupPRG::from_aes_seed((msg_size - 1) / 31 + 1, seed);
        let dpf: MultiKeyDPF<GroupPRG> = MultiKeyDPF::new(prg, channels, groups);
        let field = field_with_security(sec_bits);
        let vdpf = FieldVDPF::new(dpf, field);
        SecureProtocol::new(vdpf)
    }
}

impl<V> Protocol for SecureProtocol<V>
where
    V: VDPF,
    <V as DPF>::Key: fmt::Debug,
    <V as DPF>::Message: From<Bytes> + Into<Bytes> + Accumulatable + Clone,
    V::Token: Clone,
    V::AuthKey: Clone,
{
    type ChannelKey = ChannelKey<V>; // channel number, password
    type WriteToken = WriteToken<V>; // message, index, maybe a key
    type AuditShare = AuditShare<V>;
    type Accumulator = <V as DPF>::Message;

    fn num_parties(&self) -> usize {
        self.vdpf.num_keys()
    }

    fn num_channels(&self) -> usize {
        self.vdpf.num_points()
    }

    fn message_len(&self) -> usize {
        self.vdpf.msg_size()
    }

    fn broadcast(&self, message: Bytes, key: ChannelKey<V>) -> Vec<WriteToken<V>> {
        let dpf_keys = self.vdpf.gen(message.into(), key.idx);
        let proof_shares = self.vdpf.gen_proofs(&key.secret, key.idx, &dpf_keys);
        dpf_keys
            .into_iter()
            .zip(proof_shares.into_iter())
            .map(|(dpf_key, proof_share)| WriteToken::new(dpf_key, proof_share))
            .collect()
    }

    fn null_broadcast(&self) -> Vec<WriteToken<V>> {
        let dpf_keys = self.vdpf.gen_empty();
        let proof_shares = self.vdpf.gen_proofs_noop(&dpf_keys);

        dpf_keys
            .into_iter()
            .zip(proof_shares.into_iter())
            .map(|(dpf_key, proof_share)| WriteToken::new(dpf_key, proof_share))
            .collect()
    }

    fn gen_audit(&self, keys: &[ChannelKey<V>], token: &WriteToken<V>) -> Vec<AuditShare<V>> {
        let auth_keys: Vec<_> = keys.iter().map(|key| key.secret.clone()).collect();
        let token = AuditShare::new(self.vdpf.gen_audit(&auth_keys, &token.0, &token.1));
        repeat(token).take(self.num_parties()).collect()
    }

    fn check_audit(&self, tokens: Vec<AuditShare<V>>) -> bool {
        assert_eq!(tokens.len(), self.num_parties());
        let tokens = tokens.into_iter().map(|t| t.token).collect();
        self.vdpf.check_audit(tokens)
    }

    fn new_accumulator(&self) -> Vec<Self::Accumulator> {
        vec![self.vdpf.null_message(); self.num_channels()]
    }

    fn to_accumulator(&self, token: WriteToken<V>) -> Vec<Self::Accumulator> {
        self.vdpf.eval(token.0)
    }
}

#[cfg(test)]
impl<V> Arbitrary for SecureProtocol<V>
where
    V: VDPF + Arbitrary + 'static,
{
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        any::<V>().prop_map(SecureProtocol::new).boxed()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    use crate::tests::*;
    use spectrum_primitives::field::FieldElement;

    impl Arbitrary for ChannelKey<BasicVdpf> {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            (
                any::<BasicVdpf>(),
                any::<prop::sample::Index>(),
                any::<FieldElement>(),
            )
                .prop_map(|(vdpf, idx, value)| Self::new(idx.index(vdpf.num_points()), value))
                .boxed()
        }
    }

    mod two_key {
        use super::*;

        proptest! {
            #[test]
            fn test_null_broadcast_passes_audit(
                protocol in any::<SecureProtocol<BasicVdpf>>(),
            ) {
                let keys = protocol.sample_keys();
                check_null_broadcast_passes_audit(protocol, keys);
            }

            #[test]
            fn test_broadcast_passes_audit(
                (protocol, msg) in any::<SecureProtocol<BasicVdpf>>().prop_flat_map(and_messages),
                idx in any::<prop::sample::Index>(),
            ) {
                let keys = protocol.sample_keys();
                let idx = idx.index(keys.len());
                check_broadcast_passes_audit(protocol, msg, keys, idx);
            }

            #[test]
            fn test_broadcast_bad_key_fails_audit(
                (protocol, msg) in any::<SecureProtocol<BasicVdpf>>().prop_flat_map(and_messages),
                idx in any::<prop::sample::Index>(),
            ) {
                prop_assume!(msg != Bytes::empty(msg.len()), "Broadcasting null message okay!");
                let keys = protocol.sample_keys();
                let bad_key = ChannelKey::new(idx.index(keys.len()), protocol.vdpf.new_access_key());
                prop_assume!(!keys.contains(&bad_key));
                check_broadcast_bad_key_fails_audit(protocol, msg, keys, bad_key);
            }

            #[test]
            fn test_null_broadcast_messages_unchanged(
                (protocol, accumulator) in any::<SecureProtocol<BasicVdpf>>().prop_flat_map(and_accumulators)
            ) {
                check_null_broadcast_messages_unchanged(protocol, accumulator);
            }

            #[test]
            fn test_broadcast_recovers_message(
                (protocol, msg) in any::<SecureProtocol<BasicVdpf>>().prop_flat_map(and_messages),
                idx in any::<prop::sample::Index>(),
            ) {
                let keys = protocol.sample_keys();
                let idx = idx.index(keys.len());
                check_broadcast_recovers_message(protocol, msg, keys, idx);
            }
        }
    }

    mod multi_key {
        use super::*;

        proptest! {
            #[test]
            fn test_null_broadcast_passes_audit(
                protocol in any::<SecureProtocol<MultiKeyVdpf>>(),
            ) {
                let keys = protocol.sample_keys();
                check_null_broadcast_passes_audit(protocol, keys);
            }

            #[test]
            fn test_broadcast_passes_audit(
                (protocol, msg) in any::<SecureProtocol<MultiKeyVdpf>>().prop_flat_map(and_messages),
                idx in any::<prop::sample::Index>(),
            ) {
                let keys = protocol.sample_keys();
                let idx = idx.index(keys.len());
                check_broadcast_passes_audit(protocol, msg, keys, idx);
            }

            #[test]
            fn test_broadcast_bad_key_fails_audit(
                (protocol, msg) in any::<SecureProtocol<MultiKeyVdpf>>().prop_flat_map(and_messages),
                idx in any::<prop::sample::Index>(),
            ) {
                prop_assume!(msg != Bytes::empty(msg.len()), "Broadcasting null message okay!");
                let keys = protocol.sample_keys();
                let bad_key = ChannelKey::new(idx.index(keys.len()), protocol.vdpf.new_access_key());
                prop_assume!(!keys.contains(&bad_key));
                check_broadcast_bad_key_fails_audit(protocol, msg, keys, bad_key);
            }

            #[test]
            fn test_null_broadcast_messages_unchanged(
                (protocol, accumulator) in any::<SecureProtocol<MultiKeyVdpf>>().prop_flat_map(and_accumulators)
            ) {
                check_null_broadcast_messages_unchanged(protocol, accumulator);
            }

            #[test]
            fn test_broadcast_recovers_message(
                (protocol, msg) in any::<SecureProtocol<MultiKeyVdpf>>().prop_flat_map(and_messages),
                idx in any::<prop::sample::Index>(),
            ) {
                let keys = protocol.sample_keys();
                let idx = idx.index(keys.len());
                check_broadcast_recovers_message(protocol, msg, keys, idx);
            }
        }
    }

    proptest! {
        #[test]
        #[cfg(feature = "proto")]
        fn test_write_token_proto_roundtrip(token in any::<WriteToken<BasicVdpf>>()) {
            let wrapped: proto::WriteToken = token.clone().into();
            assert_eq!(token, wrapped.try_into().unwrap());
        }

        #[test]
        #[cfg(feature = "proto")]
        fn test_audit_share_proto_roundtrip(share in any::<AuditShare<BasicVdpf>>()) {
            let wrapped: proto::AuditShare = share.clone().into();
            assert_eq!(share, wrapped.try_into().unwrap());
        }
    }

    proptest! {
        #[test]
        #[cfg(feature = "proto")]
        fn test_multi_key_write_token_proto_roundtrip(token in any::<WriteToken<MultiKeyVdpf>>()) {
            let wrapped: proto::WriteToken = token.clone().into();
            assert_eq!(token, wrapped.try_into().unwrap());
        }

        #[test]
        #[cfg(feature = "proto")]
        fn test_multi_key_audit_share_proto_roundtrip(share in any::<AuditShare<MultiKeyVdpf>>()) {
            let wrapped: proto::AuditShare = share.clone().into();
            assert_eq!(share, wrapped.try_into().unwrap());
        }
    }
}
