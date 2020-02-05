//! Spectrum implementation.
#![allow(dead_code)]

extern crate rand;
use crate::crypto::dpf::{DPFKey, PRGBasedDPF};
use crate::crypto::field::{Field, FieldElement};
use crate::crypto::lss::{SecretShare, LSS};
use rug::{rand::RandState, Integer};
use std::rc::Rc;

#[derive(Clone, PartialEq, Debug)]
struct CryptoParams {
    num_channels: usize,
    num_servers: usize,
    dpf: PRGBasedDPF,
    field: Rc<Field>,
}

// TODO: make sure it matches with protobufs
#[derive(Clone, PartialEq, Debug)]
struct ServerAuditToken {
    bit_check_token: SecretShare,
    seed_check_token: SecretShare,
    msg_check_token: FieldElement,
}

#[derive(Clone, PartialEq, Debug)]
struct ClientProofShare {
    bit_proof_share: SecretShare,
    seed_proof_share: SecretShare,
}

impl CryptoParams {
    pub fn new(
        num_channels: usize,
        num_servers: usize,
        dpf: PRGBasedDPF,
        field: Rc<Field>,
    ) -> CryptoParams {
        CryptoParams {
            num_channels,
            num_servers,
            dpf,
            field,
        }
    }

    /// generates an audit token based on the provided DPF key
    /// and proof share
    pub fn gen_audit_token(
        &self,
        channel_keys: Vec<FieldElement>,
        dpf_key: DPFKey,
        client_proof: ClientProofShare,
    ) -> ServerAuditToken {
        assert_eq!(channel_keys.len(), dpf_key.bits.len());
        assert_eq!(channel_keys.len(), dpf_key.seeds.len());

        let mut res_seed = FieldElement::zero(self.field.clone());
        let mut res_bit = FieldElement::zero(self.field.clone());

        for (i, (seed, bit)) in dpf_key.seeds.iter().zip(dpf_key.bits.iter()).enumerate() {
            assert!(*bit == 0 || *bit == 1);

            res_seed -= channel_keys[i].clone()
                * FieldElement::from_bytes(seed.raw_bytes(), self.field.clone());

            if *bit == 1 {
                res_bit += channel_keys[i].clone();
            }
        }

        let mut bit_check_token = client_proof.bit_proof_share.clone();
        let mut seed_check_token = client_proof.seed_proof_share;
        bit_check_token.scalar_add(res_bit);
        seed_check_token.scalar_add(res_seed);

        // TODO(sss): actually hash the message?
        let msg_check_token = FieldElement::from_bytes(dpf_key.encoded_msg, self.field.clone());

        // evaluate the compressed DPF for the given dpf_key
        ServerAuditToken {
            bit_check_token,
            seed_check_token,
            msg_check_token,
        }
    }

    /// checks that the set of audit tokens sums to zero in which case
    /// the audit succeeds
    pub fn check_audit(&self, token_a: ServerAuditToken, token_b: ServerAuditToken) -> bool {
        // let sum_bits = token_a.bit_check_token - token_b.bit_check_token;
        // let sum_seeds = token_a.seed_check_token - token_b.seed_check_token;

        let bit_check = LSS::recover(vec![token_a.bit_check_token, token_b.bit_check_token]);
        let seed_check = LSS::recover(vec![token_a.seed_check_token, token_b.seed_check_token]);

        // check if sum is all-zero
        bit_check == FieldElement::zero(self.field.clone())
            && seed_check == FieldElement::zero(self.field.clone())
            && (token_a.msg_check_token == token_b.msg_check_token)
    }

    /// generates secret shares for the proof attesting to the correctness of the
    /// DPF keys generateed
    pub fn gen_proof_shares(
        &self,
        channel_index: usize,
        channel_key: FieldElement,
        dpf_keys: Vec<DPFKey>,
    ) -> Vec<ClientProofShare> {
        // TODO(sss): move this to DPF

        let dpf_key_a = dpf_keys[0].clone();
        let dpf_key_b = dpf_keys[1].clone();

        let mut res_seed_a = FieldElement::zero(self.field.clone());
        let mut res_seed_b = FieldElement::zero(self.field.clone());

        /* 1) generate the proof using the DPF keys and the channel key */

        let mut proof_correction = 1;

        for (i, (seed, bit)) in dpf_key_a
            .seeds
            .iter()
            .zip(dpf_key_a.bits.iter())
            .enumerate()
        {
            assert!(*bit == 0 || *bit == 1);
            res_seed_a += FieldElement::from_bytes(seed.raw_bytes(), self.field.clone());

            if i == channel_index && *bit == 1 {
                proof_correction = -1;
            }
        }

        for (seed, bit) in dpf_key_b.seeds.iter().zip(dpf_key_b.bits.iter()) {
            assert!(*bit == 0 || *bit == 1);
            res_seed_b += FieldElement::from_bytes(seed.raw_bytes(), self.field.clone());
        }

        /* 2) split the proof into secret shares */

        let bit_proof = channel_key.clone()
            * FieldElement::new(Integer::from(proof_correction), self.field.clone());
        let seed_proof = channel_key * (res_seed_a - res_seed_b);

        let mut rng = RandState::new();
        let bit_proof_shares = LSS::share(bit_proof, dpf_keys.len(), &mut rng);
        let seed_proof_shares = LSS::share(seed_proof, dpf_keys.len(), &mut rng);
        let mut proof_shares: Vec<ClientProofShare> = Vec::new();

        for (bit_proof_share, seed_proof_share) in
            bit_proof_shares.iter().zip(seed_proof_shares.iter())
        {
            proof_shares.push(ClientProofShare {
                bit_proof_share: (*bit_proof_share).clone(),
                seed_proof_share: (*seed_proof_share).clone(),
            });
        }

        proof_shares
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use crate::crypto::dpf::DPF;

    fn random_field() -> Rc<Field> {
        let mut p = Integer::from(800_000_000);
        p.next_prime_mut();
        Rc::<Field>::new(Field::new(p))
    }
    #[test]
    fn test_audit_check_correct() {
        let mut rng = RandState::new();

        let num_chan = 100;
        let num_servers = 2;
        let chan_idx = 5;
        let sec_bytes = 16;
        let field = random_field();

        let channel_keys = vec![FieldElement::rand_element(&mut rng, field.clone()); num_chan];
        let dpf = PRGBasedDPF::new(sec_bytes, num_servers, num_chan);
        let dpf_keys = dpf.gen(Bytes::from(vec![0; sec_bytes * 10]), chan_idx);
        let params = CryptoParams::new(num_chan, num_servers, dpf, field);

        let client_proof_shares =
            params.gen_proof_shares(chan_idx, channel_keys[chan_idx].clone(), dpf_keys.clone());

        let token_a = params.gen_audit_token(
            channel_keys.clone(),
            dpf_keys[0].clone(),
            client_proof_shares[0].clone(),
        );
        let token_b = params.gen_audit_token(
            channel_keys,
            dpf_keys[1].clone(),
            client_proof_shares[1].clone(),
        );

        assert_eq!(params.check_audit(token_a, token_b), true);
    }
}
