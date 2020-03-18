use crate::proto::{self, AuditShare, WriteToken};
use crate::protocols::{Accumulatable, Protocol};
use crate::crypto::field::{Field, FieldElement};
use crate::crypto::dpf::{DPF, DPFKey, PRGBasedDPF};
use crate::crypto::vdpf::{VDPF, PRGBasedVDPF, PRGProofShare, PRGAuditToken};
use std::convert::TryInto;
use bytes::Bytes;
use std::rc::Rc;
use rug::{rand::RandState, Integer};

#[derive(Debug, Clone)]
pub struct SecureChannelKey(usize, FieldElement);

impl SecureChannelKey {
    pub fn new(idx: usize, key: FieldElement) -> Self {
        SecureChannelKey(idx, key)
    }
}

#[derive(Debug, Clone)]
pub struct SecureWriteToken(DPFKey, PRGProofShare);

impl SecureWriteToken {
    fn new(key: DPFKey, proof_share: PRGProofShare) -> Self {
        SecureWriteToken(key, proof_share)
    }
}

#[derive(Debug, Clone)]
pub struct SecureAuditShare(PRGAuditToken);

impl SecureAuditShare {
    fn new(token: PRGAuditToken) -> Self {
        SecureAuditShare(token)
    }
}


impl Accumulatable for u8 {
    fn accumulate(&mut self, rhs: Self) {
        *self ^= rhs;
    }

    fn new(size: usize) -> Self {
        assert_eq!(size, 1);
        Default::default()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SecureProtocol {
    msg_size: usize,
    parties: usize,
    channels: usize,
    sec_bytes: usize,
    field: Rc::<Field>, 
    dpf: PRGBasedDPF,
}

impl SecureProtocol {
    pub fn new(sec_bytes: usize, msg_size: usize,  parties: usize, channels: usize) -> SecureProtocol {
        // generate random authentication keys for the vdpf
        let mut rng = RandState::new();
        // construct dpf with the parameters of the SecureProtocol
        let dpf = PRGBasedDPF::new(sec_bytes, parties, channels);


        // make field
        // TODO(sss) [important]: sample random prime of the correct security bytes
        let mut p = Integer::from(100_000_000_000_000_000_000_000);
        p.next_prime_mut();        
        let field = Rc::<Field>::new(Field::new(p));

        SecureProtocol { msg_size, parties, channels, sec_bytes, dpf, field}
    }
}

impl Protocol for SecureProtocol {
    type Message = Bytes;
    type ChannelKey = SecureChannelKey; // channel number, password
    type WriteToken = SecureWriteToken; // message, index, maybe a key
    type AuditShare = SecureAuditShare;
    type Accumulator = Vec<Bytes>;

    fn num_parties(&self) -> usize {
        self.parties
    }

    fn num_channels(&self) -> usize {
        self.channels
    }

    fn sec_bytes(&self) -> usize {
        self.sec_bytes
    }

    fn broadcast(&self, message: Bytes, channel_key: SecureChannelKey) -> Vec<SecureWriteToken> {
        // generate dpf keys for the message and index
        let dpf_keys = self.dpf.gen(message, channel_key.0); // channel_key.0 = index 

        // channel_key.0 = index,  channel_key.1 = access key for index
        // generate the proof shares
        let vdpf = PRGBasedVDPF::new(&self.dpf);
        let proof_shares = vdpf.gen_proofs(&channel_key.1, channel_key.0, &dpf_keys); 

        // generate and return the write tokens
        let write_tokens = dpf_keys.iter().zip(proof_shares.iter()).map(|(&dpf_key, &proof_share)| {
            SecureWriteToken::new(dpf_key, proof_share)
        }).collect();

        write_tokens
    }

    fn null_broadcast(&self) -> Vec<SecureWriteToken> {
         // generate dpf keys for the message and index
         // HACK: setting DPF index = num_channels creates NULL DPF 
         // TODO(sss): make a NULL flag which generates the zero DPF
         let dpf_keys = self.dpf.gen(Bytes::from(vec![0; self.msg_size]), self.num_channels()); 
 
         // generate the proof shares
         let null_element = FieldElement::new(0.into(), self.field);
         
         // HACK: as above
         let vdpf = PRGBasedVDPF::new(&self.dpf);
         let proof_shares = vdpf.gen_proofs(&null_element, self.num_channels(), &dpf_keys); 
 
         // generate and return the write tokens
         let write_tokens = dpf_keys.iter().zip(proof_shares.iter()).map(|(&dpf_key, &proof_share)| {
             SecureWriteToken::new(dpf_key, proof_share)
            }).collect();

         write_tokens
    }

    fn gen_audit(
        &self,
        keys: &[SecureChannelKey],
        token: &SecureWriteToken,
    ) -> SecureAuditShare {

        let auth_keys: Vec::<FieldElement> = Vec::new();
        keys.iter().map(|key| {
            auth_keys.push(key.1);
        });

        let vdpf = PRGBasedVDPF::new(&self.dpf);
        let audit_token = vdpf.gen_audit(&auth_keys, &token.0, &token.1);
       
        SecureAuditShare::new(audit_token)
    }

    fn check_audit(&self, tokens: Vec<SecureAuditShare>) -> bool {
        assert_eq!(tokens.len(), self.parties);
        let vdpf = PRGBasedVDPF::new(&self.dpf);
        let audit_tokens = tokens.iter().map(|token|{
            token.0
        }).collect();

        vdpf.check_audit(audit_tokens)
    }

}

