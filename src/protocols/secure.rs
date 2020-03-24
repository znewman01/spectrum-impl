use crate::proto;
use crate::{
    bytes::Bytes,
    crypto::{
        dpf::{PRGKey, DPF, PRGDPF},
        field::Field,
        prg::AESPRG,
        vdpf::{FieldProofShare, FieldToken, FieldVDPF, VDPF},
    },
    protocols::Protocol,
};
use derivative::Derivative;
use rug::Integer;
use std::convert::TryFrom;
use std::fmt;
use std::iter::repeat;
use std::sync::Arc;

pub use crate::crypto::vdpf::ConcreteVdpf;

#[derive(Derivative)]
#[derivative(
    Clone(bound = "V::AuthKey: Clone"),
    Debug(bound = "V::AuthKey: fmt::Debug"),
    PartialEq(bound = "V::AuthKey: PartialEq"),
    Eq(bound = "V::AuthKey: Eq")
)]
pub struct ChannelKey<V: VDPF> {
    pub(in crate::protocols) idx: usize,
    pub(in crate::protocols) secret: V::AuthKey,
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

impl Into<proto::WriteToken> for WriteToken<ConcreteVdpf> {
    fn into(self) -> proto::WriteToken {
        let dpf_key_proto = proto::secure_write_token::DpfKey {
            encoded_msg: self.0.encoded_msg.into(),
            bits: self.0.bits,
            seeds: self.0.seeds.into_iter().map(|s| s.into()).collect(),
        };
        let bit = self.1.bit.value();
        let modulus: proto::Integer = bit.field().into();
        let proof = proto::secure_write_token::ProofShare {
            bit: Some(bit.into()),
            seed: Some(self.1.seed.value().into()),
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

impl TryFrom<proto::WriteToken> for WriteToken<ConcreteVdpf> {
    type Error = &'static str;

    fn try_from(token: proto::WriteToken) -> Result<Self, Self::Error> {
        if let proto::write_token::Inner::Secure(inner) = token.inner.unwrap() {
            let key_proto = inner.key.unwrap();
            let dpf_key = PRGKey::<AESPRG>::new(
                key_proto.encoded_msg.into(),
                key_proto.bits,
                key_proto.seeds.into_iter().map(|s| s.into()).collect(),
            );
            let modulus: proto::Integer = inner.modulus.unwrap();
            let field = Arc::new(Field::from(modulus));
            let proof_proto = inner.proof.unwrap();
            let proof_share = FieldProofShare::new(
                field.from_proto(proof_proto.bit.unwrap()).into(),
                field.from_proto(proof_proto.seed.unwrap()).into(),
            );
            Ok(WriteToken::<ConcreteVdpf>(dpf_key, proof_share))
        } else {
            Err("Invalid proto::WriteToken.")
        }
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

impl Into<proto::AuditShare> for AuditShare<ConcreteVdpf> {
    fn into(self) -> proto::AuditShare {
        let bit = self.token.bit.value();
        let field = bit.field();
        let modulus: proto::Integer = field.into();

        let inner = proto::audit_share::Inner::Secure(proto::SecureAuditShare {
            bit: Some(bit.into()),
            seed: Some(self.token.seed.value().into()),
            data: self.token.data,
            modulus: Some(modulus),
        });
        proto::AuditShare { inner: Some(inner) }
    }
}

impl TryFrom<proto::AuditShare> for AuditShare<ConcreteVdpf> {
    type Error = &'static str;

    fn try_from(share: proto::AuditShare) -> Result<Self, Self::Error> {
        if let proto::audit_share::Inner::Secure(inner) = share.inner.unwrap() {
            let modulus: proto::Integer = inner.modulus.unwrap();
            let field = Arc::new(Field::from(modulus));

            let bit = field.from_proto(inner.bit.unwrap());
            let seed = field.from_proto(inner.seed.unwrap());

            Ok(Self::new(FieldToken::new(
                bit.into(),
                seed.into(),
                inner.data,
            )))
        } else {
            Err("Invalid proto::AuditShare.")
        }
    }
}

#[derive(Clone, Debug)]
pub struct SecureProtocol<V> {
    msg_size: usize,
    pub(in crate) vdpf: V,
}

impl<V: VDPF> SecureProtocol<V> {
    pub fn new(vdpf: V, msg_size: usize) -> SecureProtocol<V> {
        SecureProtocol { msg_size, vdpf }
    }

    #[allow(dead_code)]
    fn sample_keys(&self) -> Vec<ChannelKey<V>> {
        self.vdpf
            .sample_keys()
            .into_iter()
            .enumerate()
            .map(|(idx, secret)| ChannelKey::<V>::new(idx, secret))
            .collect()
    }
}

impl SecureProtocol<ConcreteVdpf> {
    pub fn with_aes_prg_dpf(
        sec_bytes: u32,
        parties: usize,
        channels: usize,
        msg_size: usize,
    ) -> Self {
        let prime: Integer = (Integer::from(2) << sec_bytes).next_prime_ref().into();
        let field = Field::from(prime);
        let vdpf = FieldVDPF::new(
            PRGDPF::new(AESPRG::new(16, msg_size), parties, channels),
            field,
        );
        SecureProtocol::new(vdpf, msg_size)
    }
}

impl<V> Protocol for SecureProtocol<V>
where
    V: VDPF,
    <V as DPF>::Key: Debug,
    <V as DPF>::Message: From<Bytes> + Into<Bytes>,
    V::Token: Clone,
    V::AuthKey: Clone,
{
    type ChannelKey = ChannelKey<V>; // channel number, password
    type WriteToken = WriteToken<V>; // message, index, maybe a key
    type AuditShare = AuditShare<V>;

    fn num_parties(&self) -> usize {
        self.vdpf.num_keys()
    }

    fn num_channels(&self) -> usize {
        self.vdpf.num_points()
    }

    fn message_len(&self) -> usize {
        self.msg_size
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
        let dpf_keys = self.vdpf.gen_empty(self.msg_size);
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
        let _ = token.clone();
        repeat(token).take(self.num_parties()).collect()
    }

    fn check_audit(&self, tokens: Vec<AuditShare<V>>) -> bool {
        assert_eq!(tokens.len(), self.num_parties());
        let tokens = tokens.into_iter().map(|t| t.token).collect();
        self.vdpf.check_audit(tokens)
    }

    fn to_accumulator(&self, token: WriteToken<V>) -> Vec<Bytes> {
        self.vdpf
            .eval(&token.0)
            .into_iter()
            .map(Into::into)
            .collect()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use proptest::prelude::*;

    use std::convert::TryInto;

    use crate::crypto::field::FieldElement;
    use crate::protocols::tests::*;

    impl<V> Arbitrary for SecureProtocol<V>
    where
        V: VDPF + Arbitrary,
        <V as Arbitrary>::Strategy: 'static,
    {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            any::<V>()
                .prop_map(|vdpf| SecureProtocol::new(vdpf, MSG_LEN))
                .boxed()
        }
    }

    impl Arbitrary for ChannelKey<ConcreteVdpf> {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            (
                any::<ConcreteVdpf>(),
                any::<prop::sample::Index>(),
                any::<FieldElement>(),
            )
                .prop_map(|(vdpf, idx, value)| Self::new(idx.index(vdpf.num_points()), value))
                .boxed()
        }
    }

    proptest! {
        #[test]
        fn test_null_broadcast_passes_audit(
            protocol in any::<SecureProtocol<ConcreteVdpf>>(),
        ) {
            let keys = protocol.sample_keys();
            check_null_broadcast_passes_audit(protocol, keys);
        }

        #[test]
        fn test_broadcast_passes_audit(
            protocol in any::<SecureProtocol<ConcreteVdpf>>(),
            msg in messages(),
            idx in any::<prop::sample::Index>(),
        ) {
            let keys = protocol.sample_keys();
            let idx = idx.index(keys.len());
            check_broadcast_passes_audit(protocol, msg, keys, idx);
        }

        #[test]
        fn test_broadcast_bad_key_fails_audit(
            protocol in any::<SecureProtocol<ConcreteVdpf>>(),
            msg in messages().prop_filter("Broadcasting null message okay!", |m| *m != Bytes::empty(MSG_LEN)),
            idx in any::<prop::sample::Index>(),
        ) {
            let keys = protocol.sample_keys();
            let bad_key = ChannelKey::new(idx.index(keys.len()), protocol.vdpf.sample_key());
            prop_assume!(!keys.contains(&bad_key));
            check_broadcast_bad_key_fails_audit(protocol, msg, keys, bad_key);
        }

        #[test]
        fn test_null_broadcast_messages_unchanged(
            (protocol, accumulator) in any::<SecureProtocol<ConcreteVdpf>>().prop_flat_map(and_accumulators)
        ) {
            check_null_broadcast_messages_unchanged(protocol, accumulator);
        }

        #[test]
        fn test_broadcast_recovers_message(
            protocol in any::<SecureProtocol<ConcreteVdpf>>(),
            msg in messages(),
            idx in any::<prop::sample::Index>(),
        ) {
            let keys = protocol.sample_keys();
            let idx = idx.index(keys.len());
            check_broadcast_recovers_message(protocol, msg, keys, idx);
        }
    }

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

    proptest! {
        #[test]
        fn test_write_token_proto_roundtrip(token in any::<WriteToken<ConcreteVdpf>>()) {
            let wrapped: proto::WriteToken = token.clone().into();
            assert_eq!(token, wrapped.try_into().unwrap());
        }

        #[test]
        fn test_audit_share_proto_roundtrip(share in any::<AuditShare<ConcreteVdpf>>()) {
            let wrapped: proto::AuditShare = share.clone().into();
            assert_eq!(share, wrapped.try_into().unwrap());
        }
    }
}
