//! Spectrum implementation.
#![allow(dead_code)]
use crate::bytes::Bytes;
use crate::crypto::{
    dpf::{DPF, PRGDPF},
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
pub struct FieldToken {
    bit: SecretShare,
    seed: SecretShare,
    data: FieldElement,
}

#[derive(Clone, PartialEq, Debug)]
pub struct FieldProofShare {
    bit: SecretShare,
    seed: SecretShare,
}

impl FieldProofShare {
    fn share(
        bit_proof: FieldElement,
        seed_proof: FieldElement,
        len: usize,
    ) -> Vec<FieldProofShare> {
        let mut rng = RandState::new();
        let bits = LSS::share(bit_proof, len, &mut rng);
        let seeds = LSS::share(seed_proof, len, &mut rng);
        bits.into_iter()
            .zip(seeds.into_iter())
            .map(|(bit, seed)| FieldProofShare { bit, seed })
            .collect()
    }
}

#[derive(Debug)]
pub struct FieldVDPF<D> {
    dpf: D,
    field: Rc<Field>,
}

impl<D> FieldVDPF<D> {
    pub fn new(dpf: D, field: Rc<Field>) -> Self {
        FieldVDPF { dpf, field }
    }
}

// Pass through DPF methods
impl<D: DPF> DPF for FieldVDPF<D> {
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

impl VDPF for FieldVDPF<PRGDPF<AESPRG>> {
    type AuthKey = FieldElement;
    type ProofShare = FieldProofShare;
    type Token = FieldToken;

    fn gen_proofs(
        &self,
        auth_key: &FieldElement,
        point_idx: usize,
        dpf_keys: &[<Self as DPF>::Key],
    ) -> Vec<FieldProofShare> {
        assert_eq!(dpf_keys.len(), 2, "not implemented");

        let dpf_key_a = dpf_keys[0].clone();
        let dpf_key_b = dpf_keys[1].clone();

        // 1) generate the proof using the DPF keys and the channel key
        let res_seed = dpf_key_a
            .seeds
            .iter()
            .map(|s| s.to_field_element(self.field.clone()))
            .fold(self.field.zero(), |x, y| x + y);
        let res_seed = dpf_key_b
            .seeds
            .iter()
            .map(|s| s.to_field_element(self.field.clone()))
            .fold(res_seed, |x, y| x - y);

        // If the bit is `1` for server A, then we need to negate the share
        // to ensure that (bit_A - bit_B = 1)*key = -key and not key
        let bit_proof = if dpf_key_a.bits[point_idx] == 1 {
            -auth_key.clone()
        } else {
            auth_key.clone()
        };
        let seed_proof = auth_key.clone() * res_seed;

        FieldProofShare::share(bit_proof, seed_proof, self.num_keys())
    }

    fn gen_proofs_noop(&self, dpf_keys: &[<Self as DPF>::Key]) -> Vec<Self::ProofShare> {
        self.gen_proofs(&self.field.zero(), self.num_points(), dpf_keys)
    }

    fn gen_audit(
        &self,
        auth_keys: &[FieldElement],
        dpf_key: &<Self as DPF>::Key,
        proof_share: &FieldProofShare,
    ) -> FieldToken {
        let mut bit_check = proof_share.bit.clone();
        let mut seed_check = proof_share.seed.clone();
        for ((key, seed), bit) in auth_keys
            .iter()
            .zip(dpf_key.seeds.iter())
            .zip(dpf_key.bits.iter())
        {
            seed_check -= key.clone() * seed.to_field_element(self.field.clone());
            match *bit {
                0 => {}
                1 => bit_check += key.clone(),
                _ => panic!("Bit must be 0 or 1"),
            }
        }

        // TODO(sss): actually hash the message?
        let data = self.field.from_bytes(&dpf_key.encoded_msg);

        FieldToken {
            bit: bit_check,
            seed: seed_check,
            data,
        }
    }

    fn check_audit(&self, tokens: Vec<FieldToken>) -> bool {
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

    fn aes_prg_vdpfs() -> impl Strategy<Value = FieldVDPF<PRGDPF<AESPRG>>> {
        (aes_prg_dpfs(), fields()).prop_map(|(dpf, field)| FieldVDPF::new(dpf, Rc::new(field)))
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

    fn run_test_audit_check_correct_for_noop<V>(vdpf: V, auth_keys: &[V::AuthKey], data_size: usize)
    where
        V: VDPF,
    {
        let dpf_keys = vdpf.gen(&Bytes::empty(data_size), vdpf.num_points());
        let proof_shares = vdpf.gen_proofs_noop(&dpf_keys);
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

        fn test_audit_check_correct_for_noop(
            vdpf in aes_prg_vdpfs(),
            data_size in DATA_SIZE,
        ) {
            let num_points = vdpf.num_points();
            let auth_keys = make_auth_keys(num_points, vdpf.field.clone());

            run_test_audit_check_correct_for_noop(vdpf, &auth_keys, data_size);
        }

    }
}
