//! Spectrum implementation.
#![allow(dead_code)]

extern crate rand;
use crate::crypto::byte_utils::{xor_bytes, xor_bytes_list};
use crate::crypto::dpf::{DPFKey, PRGBasedDPF, DPF};
use crate::crypto::field::{Field, FieldElement};
use bytes::Bytes;
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
    bit_check_token: FieldElement,
    seed_check_token: FieldElement,
}

#[derive(Clone, PartialEq, Debug)]
struct ClientProofShare {
    bit_proof_share: FieldElement,
    seed_proof_share: FieldElement,
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

            res_seed += channel_keys[i].clone()
                * FieldElement::from_bytes(seed.raw_bytes(), self.field.clone());

            if *bit == 1 {
                res_bit += channel_keys[i].clone();
            }
        }

        // TODO(sss): check the hash of the encoded message


        // evaluate the compressed DPF for the given dpf_key
        ServerAuditToken {
            bit_check_token:  client_proof.bit_proof_share + res_bit,
            seed_check_token: client_proof.seed_proof_share - res_seed,
        }
    }

    /// checks that the set of audit tokens sums to zero in which case
    /// the audit succeeds
    pub fn check_audit(&self, token_a: ServerAuditToken, token_b: ServerAuditToken) -> bool {
        let sum_bits = token_a.bit_check_token - token_b.bit_check_token;
        let sum_seeds = token_a.seed_check_token - token_b.seed_check_token;

        // check if sum is all-zero
        sum_seeds == FieldElement::zero(self.field.clone())
            && sum_bits == FieldElement::zero(self.field.clone())
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

        let mut proof_correction = 1;
        
        for (i, (seed, bit)) in dpf_key_a.seeds.iter().zip(dpf_key_a.bits.iter()).enumerate() {
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
        
        let proof_bits = channel_key.clone() * FieldElement::new(Integer::from(proof_correction), self.field.clone()); 
        let proof_seeds = channel_key * (res_seed_a.clone() - res_seed_b.clone());

        // TODO(sss): move this to a Share function or something.
        let mut shares: Vec<ClientProofShare> = Vec::new();

        // sum of the random field elements
        let mut rand_sum_bits = FieldElement::zero(self.field.clone());
        let mut rand_sum_seeds = FieldElement::zero(self.field.clone());
        let mut rng = RandState::new();

        for _ in 0..self.num_servers - 1 {
            shares.push(ClientProofShare {
                bit_proof_share: FieldElement::rand_element(&mut rng, self.field.clone()),
                seed_proof_share: FieldElement::rand_element(&mut rng, self.field.clone()),
            });

            // add the random shares to the sum total
            rand_sum_bits += shares.last().unwrap().bit_proof_share.clone();
            rand_sum_seeds += shares.last().unwrap().seed_proof_share.clone();
        }

        // last share set to proof + SUM rand
        shares.insert(0, ClientProofShare {
            bit_proof_share: proof_bits + rand_sum_bits,
            seed_proof_share: proof_seeds + rand_sum_seeds,
        });

        shares
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let dpf_keys = dpf.gen(Bytes::from(vec![0; sec_bytes * 2]), chan_idx);
        let params = CryptoParams::new(num_chan, num_servers, dpf, field);

        let client_proof_shares =
            params.gen_proof_shares(chan_idx, channel_keys[chan_idx].clone(), dpf_keys.clone());

        let token_a = params.gen_audit_token(channel_keys.clone(), dpf_keys[0].clone(), client_proof_shares[0].clone());
        let token_b = params.gen_audit_token(channel_keys.clone(), dpf_keys[1].clone(), client_proof_shares[1].clone());

        
        assert_eq!(params.check_audit(token_a, token_b), true);
    }
}
