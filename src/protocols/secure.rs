use crate::bytes::Bytes;
use crate::crypto::{
    dpf::{PRGBasedDPF, DPF},
    field::Field,
    prg::AESPRG,
    vdpf::{DPFVDPF, VDPF},
};
use crate::protocols::Protocol;

use rug::Integer;
use std::fmt::Debug;
use std::iter::repeat;
use std::rc::Rc;

#[derive(Debug)]
pub struct ChannelKey<V: VDPF> {
    idx: usize,
    secret: V::AuthKey,
}

impl<V: VDPF> ChannelKey<V> {
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

#[derive(Debug)]
pub struct SecureProtocol<V> {
    msg_size: usize,
    vdpf: V,
}

impl<V: VDPF> SecureProtocol<V> {
    pub fn new(vdpf: V, msg_size: usize) -> SecureProtocol<V> {
        SecureProtocol { msg_size, vdpf }
    }
}

impl SecureProtocol<DPFVDPF<PRGBasedDPF<AESPRG>>> {
    #[allow(dead_code)]
    fn with_aes_prg_dpf(sec_bytes: u32, parties: usize, channels: usize, msg_size: usize) -> Self {
        let prime: Integer = (Integer::from(2) << sec_bytes).next_prime_ref().into();
        let field = Rc::new(Field::from(prime));
        let vdpf = DPFVDPF::new(PRGBasedDPF::new(AESPRG::new(), parties, channels), field);
        SecureProtocol::new(vdpf, msg_size)
    }
}

impl<V> Protocol for SecureProtocol<V>
where
    V: VDPF,
    <V as DPF>::Key: Debug,
    V::Token: Clone,
    V::AuthKey: Clone,
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
        // generate dpf keys for the message and index
        // HACK: setting DPF index = num_channels creates NULL DPF
        // TODO(sss): make a NULL flag which generates the zero DPF
        let dpf_keys = self
            .vdpf
            .gen(&Bytes::empty(self.msg_size), self.num_channels());
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
