use crate::bytes::Bytes;
use crate::crypto::{
    dpf::{DPF, PRGDPF},
    field::Field,
    prg::AESPRG,
    vdpf::{FieldVDPF, VDPF},
};
use crate::protocols::Protocol;

use rug::Integer;
use std::fmt::Debug;
use std::iter::repeat;
use std::rc::Rc;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ChannelKey<V>
where
    V: VDPF,
    V::AuthKey: Debug + PartialEq,
{
    idx: usize,
    secret: V::AuthKey,
}

impl<V> ChannelKey<V>
where
    V: VDPF,
    V::AuthKey: Debug + PartialEq,
{
    pub fn new(idx: usize, secret: V::AuthKey) -> Self {
        ChannelKey { idx, secret }
    }
}

#[derive(Debug)]
pub struct WriteToken<V>(<V as DPF>::Key, V::ProofShare)
where
    V: VDPF,
    <V as DPF>::Key: Debug;

impl<V> WriteToken<V>
where
    V: VDPF,
    <V as DPF>::Key: Debug,
{
    fn new(key: <V as DPF>::Key, proof_share: V::ProofShare) -> Self {
        WriteToken(key, proof_share)
    }
}

#[derive(Clone, Debug)]
pub struct SecureProtocol<V> {
    msg_size: usize,
    vdpf: V,
}

impl<V> SecureProtocol<V>
where
    V: VDPF,
    V::AuthKey: Debug + PartialEq,
{
    pub fn new(vdpf: V, msg_size: usize) -> SecureProtocol<V> {
        SecureProtocol { msg_size, vdpf }
    }

    #[allow(dead_code)]
    fn sample_keys(&self) -> Vec<ChannelKey<V>> {
        self.vdpf
            .sample_keys()
            .into_iter()
            .enumerate()
            .map(|(idx, secret)| ChannelKey::new(idx, secret))
            .collect()
    }
}

impl SecureProtocol<FieldVDPF<PRGDPF<AESPRG>>> {
    #[allow(dead_code)]
    fn with_aes_prg_dpf(sec_bytes: u32, parties: usize, channels: usize, msg_size: usize) -> Self {
        let prime: Integer = (Integer::from(2) << sec_bytes).next_prime_ref().into();
        let field = Rc::new(Field::from(prime));
        let vdpf = FieldVDPF::new(PRGDPF::new(AESPRG::new(), parties, channels), field);
        SecureProtocol::new(vdpf, msg_size)
    }
}

impl<V> Protocol for SecureProtocol<V>
where
    V: VDPF,
    <V as DPF>::Key: Debug,
    V::Token: Clone,
    V::AuthKey: Debug + Clone + PartialEq,
{
    type ChannelKey = ChannelKey<V>; // channel number, password
    type WriteToken = WriteToken<V>; // message, index, maybe a key
    type AuditShare = V::Token;

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
        let dpf_keys = self.vdpf.gen(&message, key.idx);
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

    fn gen_audit(&self, keys: &[ChannelKey<V>], token: &WriteToken<V>) -> Vec<<V as VDPF>::Token> {
        let auth_keys: Vec<_> = keys.iter().map(|key| key.secret.clone()).collect();
        let token = self.vdpf.gen_audit(&auth_keys, &token.0, &token.1);
        repeat(token).take(self.num_parties()).collect()
    }

    fn check_audit(&self, tokens: Vec<<V as VDPF>::Token>) -> bool {
        assert_eq!(tokens.len(), self.num_parties());
        self.vdpf.check_audit(tokens)
    }

    fn to_accumulator(&self, token: WriteToken<V>) -> Vec<Bytes> {
        self.vdpf.eval(&token.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    use crate::crypto::vdpf::tests::aes_prg_vdpfs;
    use crate::protocols::tests::*;

    fn protocols() -> impl Strategy<Value = SecureProtocol<FieldVDPF<PRGDPF<AESPRG>>>> {
        aes_prg_vdpfs().prop_map(|vdpf| SecureProtocol::new(vdpf, MSG_LEN))
    }

    proptest! {
        #[test]
        fn test_null_broadcast_passes_audit(
            protocol in protocols(),
        ) {
            let keys = protocol.sample_keys();
            check_null_broadcast_passes_audit(protocol, keys);
        }

        #[test]
        fn test_broadcast_passes_audit(
            protocol in protocols(),
            msg in messages(),
            idx in any::<prop::sample::Index>(),
        ) {
            let keys = protocol.sample_keys();
            let idx = idx.index(keys.len());
            check_broadcast_passes_audit(protocol, msg, keys, idx);
        }

        #[test]
        fn test_broadcast_bad_key_fails_audit(
            protocol in protocols(),
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
            (protocol, accumulator) in protocols().prop_flat_map(and_accumulators)
        ) {
            check_null_broadcast_messages_unchanged(protocol, accumulator);
        }

        #[test]
        fn test_broadcast_recovers_message(
            protocol in protocols(),
            msg in messages(),
            idx in any::<prop::sample::Index>(),
        ) {
            let keys = protocol.sample_keys();
            let idx = idx.index(keys.len());
            check_broadcast_recovers_message(protocol, msg, keys, idx);
        }
    }
}
