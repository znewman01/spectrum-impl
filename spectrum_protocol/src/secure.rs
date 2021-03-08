use crate::{accumulator::Accumulatable, Protocol};

use serde::{Deserialize, Serialize};
use spectrum_primitives::{Dpf, Vdpf};

use std::fmt;
use std::iter::repeat;

#[cfg(any(test, feature = "testing"))]
use proptest_derive::Arbitrary;

#[cfg_attr(any(test, feature = "testing"), derive(Arbitrary))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wrapper<V> {
    vdpf: V,
}
impl<V> From<V> for Wrapper<V> {
    fn from(vdpf: V) -> Self {
        Wrapper { vdpf }
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Arbitrary))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteToken<K, P> {
    key: K,
    proof: P,
}

impl<K, P> WriteToken<K, P> {
    pub fn new(key: K, proof: P) -> Self {
        WriteToken { key, proof }
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Arbitrary))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditShare<T> {
    token: T,
}

impl<T> AuditShare<T> {
    pub fn new(token: T) -> Self {
        AuditShare { token }
    }
}

impl<V> Protocol for Wrapper<V>
where
    V: Vdpf,
    <V as Vdpf>::Token: Clone,
    <V as Dpf>::Key: fmt::Debug,
    <V as Dpf>::Message: Accumulatable + Clone,
{
    type ChannelKey = <V as Vdpf>::AuthKey;
    type WriteToken = WriteToken<<V as Dpf>::Key, <V as Vdpf>::ProofShare>;
    // Running into type system hiccups, have to give a new type.
    // For whatever reason rustc thinks that <V as Vdpf>::Token is proto::AuditToken.
    type AuditShare = AuditShare<<V as Vdpf>::Token>;
    type Accumulator = <V as Dpf>::Message;

    fn num_parties(&self) -> usize {
        self.vdpf.keys()
    }

    fn num_channels(&self) -> usize {
        self.vdpf.points()
    }

    fn message_len(&self) -> usize {
        self.vdpf.msg_size()
    }

    fn broadcast(
        &self,
        message: Self::Accumulator,
        idx: usize,
        key: Self::ChannelKey,
    ) -> Vec<Self::WriteToken> {
        let dpf_keys = self.vdpf.gen(message, idx);
        let proof_shares = self.vdpf.gen_proofs(&key, idx, &dpf_keys);
        Iterator::zip(dpf_keys.into_iter(), proof_shares.into_iter())
            .map(|(k, p)| WriteToken::new(k, p))
            .collect()
    }

    fn cover(&self) -> Vec<Self::WriteToken> {
        let dpf_keys = self.vdpf.gen_empty();
        let proof_shares = self.vdpf.gen_proofs_noop();
        Iterator::zip(dpf_keys.into_iter(), proof_shares.into_iter())
            .map(|(k, p)| WriteToken::new(k, p))
            .collect()
    }

    fn gen_audit(
        &self,
        keys: &[Self::ChannelKey],
        write_token: Self::WriteToken,
    ) -> Vec<Self::AuditShare> {
        let token = self
            .vdpf
            .gen_audit(&keys, &write_token.key, write_token.proof);
        repeat(token)
            .map(AuditShare::new)
            .take(self.num_parties())
            .collect()
    }

    fn check_audit(&self, tokens: Vec<Self::AuditShare>) -> bool {
        assert_eq!(tokens.len(), self.num_parties());
        self.vdpf
            .check_audit(tokens.into_iter().map(|x| x.token).collect())
    }

    fn new_accumulator(&self) -> Vec<Self::Accumulator> {
        vec![self.vdpf.null_message(); self.num_channels()]
    }

    fn to_accumulator(&self, token: Self::WriteToken) -> Vec<Self::Accumulator> {
        self.vdpf.eval(token.key)
    }
}

#[cfg(feature = "proto")]
use {
    crate::proto,
    spectrum_primitives::{
        ElementVector, MultiKeyKey, MultiKeyProof, MultiKeyToken, TwoKeyKey, TwoKeyProof,
        TwoKeyToken,
    },
    std::convert::{TryFrom, TryInto},
};

#[cfg(feature = "proto")]
impl<M, S> TryFrom<proto::secure_write_token::DpfKey> for TwoKeyKey<M, S>
where
    Vec<u8>: Into<M> + TryInto<S>,
{
    type Error = &'static str;

    fn try_from(proto: proto::secure_write_token::DpfKey) -> Result<Self, Self::Error> {
        let msg = proto.encoded_msg.into();
        let bits = proto
            .bits
            .into_iter()
            .map(|bytes| {
                if bytes.len() != 1 {
                    return Err(());
                }
                match bytes[0] {
                    0u8 => Ok(false),
                    1u8 => Ok(true),
                    _ => Err(()),
                }
            })
            .collect::<Result<Vec<bool>, _>>()
            .map_err(|_| "couldn't convert bits")?;
        let seeds = proto
            .seeds
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<S>, _>>()
            .map_err(|_| "couldn't convert seeds")?;
        Ok(Self::new(msg, bits, seeds))
    }
}

#[cfg(feature = "proto")]
impl<M, S> From<TwoKeyKey<M, S>> for proto::secure_write_token::DpfKey
where
    M: Clone + Into<Vec<u8>>,
    S: Clone + Into<Vec<u8>>,
{
    fn from(value: TwoKeyKey<M, S>) -> Self {
        proto::secure_write_token::DpfKey {
            encoded_msg: value.msg().into(),
            bits: value
                .bits()
                .into_iter()
                .map(Into::into)
                .map(|b| vec![b])
                .collect(),
            seeds: value.seeds().into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(feature = "proto")]
impl<M, S> TryFrom<proto::secure_write_token::DpfKey> for MultiKeyKey<M, S>
where
    Vec<u8>: TryInto<M> + TryInto<S>,
{
    type Error = ();

    fn try_from(proto: proto::secure_write_token::DpfKey) -> Result<Self, Self::Error> {
        let msg = proto.encoded_msg.try_into().map_err(|_| ())?;
        let bits = proto
            .bits
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<S>, _>>()
            .map_err(|_| ())?;
        let seeds = proto
            .seeds
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<S>, _>>()
            .map_err(|_| ())?;
        Ok(Self::new(msg, bits, seeds))
    }
}

#[cfg(feature = "proto")]
impl<M, S> From<MultiKeyKey<M, S>> for proto::secure_write_token::DpfKey
where
    M: Clone + Into<Vec<u8>>,
    S: Clone + Into<Vec<u8>>,
{
    fn from(value: MultiKeyKey<M, S>) -> Self {
        proto::secure_write_token::DpfKey {
            encoded_msg: value.msg().into(),
            bits: value.bits().into_iter().map(Into::into).collect(),
            seeds: value.seeds().into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(feature = "proto")]
impl<S> TryFrom<proto::secure_write_token::ProofShare> for TwoKeyProof<S>
where
    Vec<u8>: TryInto<S>,
{
    type Error = &'static str;

    fn try_from(proto: proto::secure_write_token::ProofShare) -> Result<Self, Self::Error> {
        let seed = proto.seed.try_into().map_err(|_| "can't convert seed")?;
        let bit = proto.bit.try_into().map_err(|_| "can't convert bit")?;
        Ok(Self::new(seed, bit))
    }
}

#[cfg(feature = "proto")]
impl<S> From<TwoKeyProof<S>> for proto::secure_write_token::ProofShare
where
    S: Clone + Into<Vec<u8>>,
{
    fn from(value: TwoKeyProof<S>) -> Self {
        proto::secure_write_token::ProofShare {
            bit: value.bit().into(),
            seed: value.seed().into(),
        }
    }
}

#[cfg(feature = "proto")]
impl<S> TryFrom<proto::secure_write_token::ProofShare> for MultiKeyProof<S>
where
    Vec<u8>: TryInto<S>,
{
    type Error = &'static str;

    fn try_from(proto: proto::secure_write_token::ProofShare) -> Result<Self, Self::Error> {
        let seed = proto.seed.try_into().map_err(|_| "can't convert seed")?;
        let bit = proto.bit.try_into().map_err(|_| "can't convert bit")?;
        Ok(Self::new(bit, seed))
    }
}

#[cfg(feature = "proto")]
impl<S> From<MultiKeyProof<S>> for proto::secure_write_token::ProofShare
where
    S: Clone + Into<Vec<u8>>,
{
    fn from(value: MultiKeyProof<S>) -> Self {
        proto::secure_write_token::ProofShare {
            bit: value.bit().into(),
            seed: value.seed().into(),
        }
    }
}

#[cfg(feature = "proto")]
impl<K, P> TryFrom<proto::WriteToken> for WriteToken<K, P>
where
    proto::secure_write_token::DpfKey: TryInto<K>,
    proto::secure_write_token::ProofShare: TryInto<P>,
{
    type Error = &'static str;

    fn try_from(value: proto::WriteToken) -> Result<Self, Self::Error> {
        // WriteToken has an optional enum for the token type; this should always be populated.
        let token_enum = value.inner.ok_or("no inner")?;
        // We expect the enum value to be a SecureWriteToken.
        if let proto::write_token::Inner::Secure(token) = token_enum {
            let key = token
                .key
                .ok_or("no key")?
                .try_into()
                .map_err(|_| "can't convert key")?;
            let proof = token
                .proof
                .ok_or("no proof")?
                .try_into()
                .map_err(|_| "can't convert proof")?;
            Ok(WriteToken::new(key, proof))
        } else {
            Err("bad")
        }
    }
}

#[cfg(feature = "proto")]
impl<K, P> From<WriteToken<K, P>> for proto::WriteToken
where
    K: Into<proto::secure_write_token::DpfKey>,
    P: Into<proto::secure_write_token::ProofShare>,
{
    fn from(value: WriteToken<K, P>) -> Self {
        // The main thing.
        let token = proto::SecureWriteToken {
            key: Some(value.key.into()),
            proof: Some(value.proof.into()),
        };
        // Stuff it in a wrapper.
        let inner = Some(proto::write_token::Inner::Secure(token));
        proto::WriteToken { inner }
    }
}

#[cfg(feature = "proto")]
impl<S> TryFrom<proto::SecureAuditShare> for TwoKeyToken<S>
where
    Vec<u8>: TryInto<S>,
{
    type Error = &'static str;

    fn try_from(proto: proto::SecureAuditShare) -> Result<Self, Self::Error> {
        let bit = proto.bit.try_into().map_err(|_| "can't convert bit")?;
        let seed = proto.seed.try_into().map_err(|_| "can't convert seed")?;
        let data = proto.data.into();
        Ok(Self::new(seed, bit, data))
    }
}

#[cfg(feature = "proto")]
impl<S> From<TwoKeyToken<S>> for proto::SecureAuditShare
where
    S: Clone + Into<Vec<u8>>,
{
    fn from(value: TwoKeyToken<S>) -> Self {
        proto::SecureAuditShare {
            bit: value.bit().into(),
            seed: value.seed().into(),
            data: value.data().into(),
        }
    }
}

#[cfg(feature = "proto")]
impl<S> TryFrom<proto::SecureAuditShare> for MultiKeyToken<S>
where
    Vec<u8>: TryInto<S>,
{
    type Error = &'static str;

    fn try_from(proto: proto::SecureAuditShare) -> Result<Self, Self::Error> {
        let bit = proto.bit.try_into().map_err(|_| "can't convert bit")?;
        let seed = proto.seed.try_into().map_err(|_| "can't convert seed")?;
        let data = proto.data.into();
        Ok(Self::new(seed, bit, data))
    }
}

#[cfg(feature = "proto")]
impl<S> From<MultiKeyToken<S>> for proto::SecureAuditShare
where
    S: Clone + Into<Vec<u8>>,
{
    fn from(value: MultiKeyToken<S>) -> Self {
        proto::SecureAuditShare {
            bit: value.bit().into(),
            seed: value.seed().into(),
            data: value.data().into(),
        }
    }
}

#[cfg(feature = "proto")]
impl<T> TryFrom<proto::AuditShare> for AuditShare<T>
where
    proto::SecureAuditShare: TryInto<T>,
{
    type Error = &'static str;

    fn try_from(value: proto::AuditShare) -> Result<Self, Self::Error> {
        // AuditShare has an optional enum for the token type; this should always be populated.
        let token_enum = value.inner.ok_or("no enum")?;
        // We expect the enum value to be a SecureAuditShare.
        if let proto::audit_share::Inner::Secure(token_proto) = token_enum {
            let token = token_proto.try_into().map_err(|_| "can't convert token")?;
            Ok(AuditShare::new(token))
        } else {
            Err("wrong type")
        }
    }
}

#[cfg(feature = "proto")]
impl<T> From<AuditShare<T>> for proto::AuditShare
where
    T: Into<proto::SecureAuditShare>,
{
    fn from(value: AuditShare<T>) -> Self {
        // The main thing.
        let token = value.token.into();
        // Stuff it in a wrapper.
        let inner = Some(proto::audit_share::Inner::Secure(token));
        proto::AuditShare { inner }
    }
}

#[cfg(feature = "proto")]
impl<G> From<Vec<ElementVector<G>>> for proto::Share
where
    ElementVector<G>: Into<Vec<u8>>,
{
    fn from(values: Vec<ElementVector<G>>) -> Self {
        proto::Share {
            data: values.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(feature = "proto")]
impl<G> TryFrom<proto::Share> for Vec<ElementVector<G>>
where
    ElementVector<G>: TryFrom<Vec<u8>>,
{
    type Error = &'static str;

    fn try_from(proto: proto::Share) -> Result<Self, Self::Error> {
        proto
            .data
            .into_iter()
            .map(ElementVector::<G>::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "conversion failed")
    }
}
