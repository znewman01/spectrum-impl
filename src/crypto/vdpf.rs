//! Spectrum implementation.
#![allow(dead_code)]
use crate::bytes::Bytes;
use crate::crypto::{
    dpf::{PRGBasedDPF, DPF},
    field::{Field, FieldElement},
    lss::{SecretShare, LSS},
    prg::AESPRG,
};
use rug::rand::RandState;
use std::fmt::Debug;
use std::rc::Rc;

// check_audit(gen_audit(gen_proof(...))) = TRUE
pub trait VDPF: DPF {
    type AuthKey;
    type ProofShare;
    type Token;

    fn gen_proofs(
        &self,
        auth_key: &Self::AuthKey,
        point_idx: usize,
        dpf_keys: &[<Self as DPF>::Key],
    ) -> Vec<Self::ProofShare>;

    fn gen_proofs_noop(&self, dpf_keys: &[<Self as DPF>::Key]) -> Vec<Self::ProofShare>;

    fn gen_audit(
        &self,
        auth_keys: &[Self::AuthKey],
        dpf_key: &<Self as DPF>::Key,
        proof_share: &Self::ProofShare,
    ) -> Self::Token;

    fn check_audit(&self, tokens: Vec<Self::Token>) -> bool;
}

#[derive(Clone, PartialEq, Debug)]
pub struct PRGAuditToken {
    bit_check_share: SecretShare,
    seed_check_share: SecretShare,
    data_check_share: FieldElement,
}

#[derive(Clone, PartialEq, Debug)]
pub struct PRGProofShare {
    bit_proof_share: SecretShare,
    seed_proof_share: SecretShare,
}

#[derive(Debug)]
pub struct DPFVDPF<D> {
    dpf: D,
    field: Rc<Field>,
}

impl<D> DPFVDPF<D> {
    pub fn new(dpf: D, field: Rc<Field>) -> Self {
        DPFVDPF { dpf, field }
    }
}

// Pass through DPF methods
impl<D: DPF> DPF for DPFVDPF<D> {
    type Key = D::Key;

    fn num_points(&self) -> usize {
        self.dpf.num_points()
    }

    fn num_keys(&self) -> usize {
        self.dpf.num_keys()
    }

    fn gen(&self, msg: &Bytes, idx: usize) -> Vec<Self::Key> {
        self.dpf.gen(msg, idx)
    }

    fn eval(&self, key: &Self::Key) -> Vec<Bytes> {
        self.dpf.eval(key)
    }

    fn combine(&self, parts: Vec<Vec<Bytes>>) -> Vec<Bytes> {
        self.dpf.combine(parts)
    }
}

impl VDPF for DPFVDPF<PRGBasedDPF<AESPRG>> {
    type AuthKey = FieldElement;
    type ProofShare = PRGProofShare;
    type Token = PRGAuditToken;

    fn gen_proofs(
        &self,
        auth_key: &FieldElement,
        point_idx: usize,
        dpf_keys: &[<Self as DPF>::Key],
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

            // If the bit is `1` for server A
            // then we need to negate the share
            // so as to ensure that (bit_A - bit_B = 1)*key = -key and not key
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

    fn gen_proofs_noop(&self, dpf_keys: &[<Self as DPF>::Key]) -> Vec<Self::ProofShare> {
        self.gen_proofs(&self.field.zero(), self.num_points(), dpf_keys)
    }

    fn gen_audit(
        &self,
        auth_keys: &[FieldElement],
        dpf_key: &<Self as DPF>::Key,
        proof_share: &PRGProofShare,
    ) -> PRGAuditToken {
        // get the field from auth keys
        let field = auth_keys
            .first()
            .expect("need at least one auth key")
            .field();

        // init to zero
        let mut res_seed = field.zero();
        let mut res_bit = field.zero();

        for (i, (seed, bit)) in dpf_key.seeds.iter().zip(dpf_key.bits.iter()).enumerate() {
            assert!(*bit == 0 || *bit == 1);

            res_seed -= auth_keys[i].clone() * seed.to_field_element(field.clone());

            if *bit == 1 {
                res_bit += auth_keys[i].clone();
            }
        }

        let mut bit_check_share = proof_share.bit_proof_share.clone();
        let mut seed_check_share = proof_share.seed_proof_share.clone();

        // TODO(sss): implement += for this!
        bit_check_share = bit_check_share + res_bit;
        seed_check_share = seed_check_share + res_seed;

        // TODO(sss): actually hash the message?
        let data_check_share = field.from_bytes(&dpf_key.encoded_msg);

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
    use crate::crypto::field::Field;
    use proptest::prelude::*;
    use rug::Integer;
    use std::iter::repeat_with;
    use std::ops::Range;
    use std::rc::Rc;

    use crate::crypto::dpf::tests::aes_prg_dpfs;

    const DATA_SIZE: Range<usize> = 100..300; // in bytes

    fn make_auth_keys(num: usize, field: Rc<Field>) -> Vec<FieldElement> {
        let mut rng = RandState::new();
        repeat_with(|| field.rand_element(&mut rng))
            .take(num)
            .collect()
    }

    fn data() -> impl Strategy<Value = Bytes> {
        prop::collection::vec(any::<u8>(), DATA_SIZE).prop_map(Bytes::from)
    }

    fn fields() -> impl Strategy<Value = Field> {
        let mut p = Integer::from(800_000_000);
        p.next_prime_mut();
        Just(p.into())
    }

    fn aes_prg_vdpfs() -> impl Strategy<Value = DPFVDPF<PRGBasedDPF<AESPRG>>> {
        (aes_prg_dpfs(), fields()).prop_map(|(dpf, field)| DPFVDPF::new(dpf, Rc::new(field)))
    }

    fn run_test_audit_check_correct<V>(
        vdpf: V,
        auth_keys: &[V::AuthKey],
        data: &Bytes,
        point_idx: usize,
    ) where
        V: VDPF,
    {
        let dpf_keys = vdpf.gen(data, point_idx);
        let proof_shares = vdpf.gen_proofs(&auth_keys[point_idx], point_idx, &dpf_keys);
        let audit_tokens = dpf_keys
            .iter()
            .zip(proof_shares.iter())
            .map(|(dpf_key, proof_share)| vdpf.gen_audit(&auth_keys, dpf_key, proof_share))
            .collect();
        assert!(vdpf.check_audit(audit_tokens));
    }

    proptest! {
        #[test]
        fn test_audit_check_correct(
            vdpf in aes_prg_vdpfs(),
            data in data(),
            point_idx in any::<proptest::sample::Index>(),
        ) {
            let num_points = vdpf.num_points();
            let point_idx = point_idx.index(num_points);
            let auth_keys = make_auth_keys(num_points, vdpf.field.clone());

            run_test_audit_check_correct(vdpf, &auth_keys, &data, point_idx);
        }

    }
}
