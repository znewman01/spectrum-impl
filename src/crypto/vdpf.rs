//! Spectrum implementation.
#![allow(dead_code)]
use crate::crypto::dpf::{DPFKey, PRGBasedDPF};
use crate::crypto::field::FieldElement;
use crate::crypto::lss::{SecretShare, LSS};
use rug::rand::RandState;
use std::fmt::Debug;

// check_audit(gen_audit(gen_proof(...))) = TRUE
pub trait VDPF<Key, AuthKey, ProofShare, Token> {
    fn gen_proofs(&self, auth_key: &AuthKey, point_idx: usize, dpf_keys: &[Key])
        -> Vec<ProofShare>;
    fn gen_audit(&self, auth_keys: &[AuthKey], dpf_key: &Key, proof_share: ProofShare) -> Token;
    fn check_audit(&self, tokens: Vec<Token>) -> bool;
}

/// DPF based on PRG
#[derive(Clone, PartialEq, Debug)]
pub struct PRGBasedVDPF<'a> {
    dpf: &'a PRGBasedDPF,
}

#[derive(Clone, PartialEq, Debug)]
struct PRGAuditToken {
    bit_check_share: SecretShare,
    seed_check_share: SecretShare,
    data_check_share: FieldElement,
}

#[derive(Clone, PartialEq, Debug)]
pub struct PRGProofShare {
    bit_proof_share: SecretShare,
    seed_proof_share: SecretShare,
}

impl PRGBasedVDPF<'_> {
    fn new(dpf: &PRGBasedDPF) -> PRGBasedVDPF {
        PRGBasedVDPF { dpf }
    }
}

impl VDPF<DPFKey, FieldElement, PRGProofShare, PRGAuditToken> for PRGBasedVDPF<'_> {
    fn gen_proofs(
        &self,
        auth_key: &FieldElement,
        point_idx: usize,
        dpf_keys: &[DPFKey],
    ) -> Vec<PRGProofShare> {
        // get the field from auth keys
        let field = auth_key.clone().field();

        assert_eq!(dpf_keys.len(), 2, "not implemented");

        let dpf_key_a = dpf_keys[0].clone();
        let dpf_key_b = dpf_keys[1].clone();

        let mut res_seed_a = field.clone().zero();
        let mut res_seed_b = field.clone().zero();

        /* 1) generate the proof using the DPF keys and the channel key */

        let mut proof_correction = 1;

        for (i, (seed, bit)) in dpf_key_a
            .seeds
            .iter()
            .zip(dpf_key_a.bits.iter())
            .enumerate()
        {
            assert!(*bit == 0 || *bit == 1);
            res_seed_a += seed.to_field_element(field.clone());

            if i == point_idx && *bit == 1 {
                proof_correction = -1;
            }
        }

        for (seed, bit) in dpf_key_b.seeds.iter().zip(dpf_key_b.bits.iter()) {
            assert!(*bit == 0 || *bit == 1);
            res_seed_b += seed.to_field_element(field.clone());
        }

        /* 2) split the proof into secret shares */

        let bit_proof = auth_key.clone() * FieldElement::new(proof_correction.into(), field);
        let seed_proof = auth_key.clone() * (res_seed_a - res_seed_b);

        let mut rng = RandState::new();
        let bit_proof_shares = LSS::share(bit_proof, dpf_keys.len(), &mut rng);
        let seed_proof_shares = LSS::share(seed_proof, dpf_keys.len(), &mut rng);
        let mut proof_shares: Vec<PRGProofShare> = Vec::new();

        for (bit_proof_share, seed_proof_share) in
            bit_proof_shares.iter().zip(seed_proof_shares.iter())
        {
            proof_shares.push(PRGProofShare {
                bit_proof_share: (*bit_proof_share).clone(),
                seed_proof_share: (*seed_proof_share).clone(),
            });
        }

        proof_shares
    }

    fn gen_audit(
        &self,
        auth_keys: &[FieldElement],
        dpf_key: &DPFKey,
        proof_share: PRGProofShare,
    ) -> PRGAuditToken {
        // get the field from auth keys
        let field = auth_keys
            .first()
            .expect("need at least one auth key")
            .clone()
            .field();

        // init to zero
        let mut res_seed = field.clone().zero();
        let mut res_bit = field.clone().zero();

        for (i, (seed, bit)) in dpf_key.seeds.iter().zip(dpf_key.bits.iter()).enumerate() {
            assert!(*bit == 0 || *bit == 1);

            res_seed -= auth_keys[i].clone() * seed.to_field_element(field.clone());

            if *bit == 1 {
                res_bit += auth_keys[i].clone();
            }
        }

        let mut bit_check_share = proof_share.bit_proof_share.clone();
        let mut seed_check_share = proof_share.seed_proof_share;

        // TODO(sss): implement += for this!
        bit_check_share = bit_check_share + res_bit;
        seed_check_share = seed_check_share + res_seed;

        // TODO(sss): actually hash the message?
        let data_check_share = FieldElement::from_bytes(&(*dpf_key).encoded_msg, field);

        // evaluate the compressed DPF for the given dpf_key
        PRGAuditToken {
            bit_check_share,
            seed_check_share,
            data_check_share,
        }
    }

    fn check_audit(&self, tokens: Vec<PRGAuditToken>) -> bool {
        assert_eq!(tokens.len(), 2, "not implemented");

        tokens[0] == tokens[1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::dpf::DPF;
    use crate::crypto::field::Field;
    use bytes::Bytes;
    use proptest::prelude::*;
    use rug::Integer;
    use std::rc::Rc;

    const MAX_NUM_POINTS: usize = 100;
    const MAX_SECURITY: usize = 100; // in bytes
    const MIN_SECURITY: usize = 16; // in bytes
    const MAX_DATA_SIZE: usize = MAX_SECURITY * 3; // in bytes
    const MIN_DATA_SIZE: usize = MAX_SECURITY; // in bytes

    fn num_points_and_point_index() -> impl Strategy<Value = (usize, usize)> {
        (1..MAX_NUM_POINTS).prop_flat_map(|num_points| (Just(num_points), 0..num_points))
    }

    fn field() -> impl Strategy<Value = Rc<Field>> {
        let mut p = Integer::from(800_000_000);
        p.next_prime_mut();
        Just(Rc::<Field>::new(Field::new(p)))
    }

    proptest! {
        #[test]
        fn test_audit_check_correct(
            (num_points, point_idx) in num_points_and_point_index(),
            num_servers in Just(2),
            sec_bytes in MIN_SECURITY..MAX_SECURITY,
            data_size_in_bytes in MIN_DATA_SIZE..MAX_DATA_SIZE,
            field in field()
        ) {
            // generate random authentication keys for the vdpf
            let mut rng = RandState::new();
            let auth_keys = vec![FieldElement::rand_element(&mut rng, field); num_points];

            let dpf = PRGBasedDPF::new(sec_bytes, num_servers, num_points);
            let vdpf = PRGBasedVDPF::new(&dpf);

            // generate dpf keys
            let dpf_keys = dpf.gen(Bytes::from(vec![0; data_size_in_bytes]), point_idx);

            let proof_shares = vdpf.gen_proofs(&auth_keys[point_idx],  point_idx, &dpf_keys);
            let audit_tokens: Vec<PRGAuditToken> = dpf_keys.iter().zip(proof_shares.into_iter()).map(|(dpf_key, proof_share)| {
                vdpf.gen_audit(&auth_keys, dpf_key, proof_share)
            }).collect();

            assert_eq!(vdpf.check_audit(audit_tokens), true);
        }

    }
}
