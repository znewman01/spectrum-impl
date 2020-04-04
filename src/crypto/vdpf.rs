//! Spectrum implementation.
use crate::crypto::{
    dpf::{DPF, PRGDPF},
    field::{Field, FieldElement},
    lss::{SecretShare, LSS},
    prg::aes::AESPRG,
};

use rug::rand::RandState;
use serde::{Deserialize, Serialize};

use std::fmt::Debug;
use std::iter::repeat_with;

// check_audit(gen_audit(gen_proof(...))) = TRUE
pub trait VDPF: DPF {
    type AuthKey;
    type ProofShare;
    type Token;

    fn sample_key(&self) -> Self::AuthKey;
    fn sample_keys(&self) -> Vec<Self::AuthKey>;

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
    pub(in crate) bit: SecretShare,
    pub(in crate) seed: SecretShare,
    pub(in crate) data: Bytes,
}

impl FieldToken {
    pub fn new(bit: SecretShare, seed: SecretShare, data: Bytes) -> Self {
        Self { bit, seed, data }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct FieldProofShare {
    pub(in crate) bit: SecretShare,
    pub(in crate) seed: SecretShare,
}

impl FieldProofShare {
    pub fn new(bit: SecretShare, seed: SecretShare) -> Self {
        Self { bit, seed }
    }

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
            .map(|(bit, seed)| FieldProofShare::new(bit, seed))
            .collect()
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct FieldVDPF<D> {
    dpf: D,
    pub(in crate) field: Field,
}

impl<D> FieldVDPF<D> {
    pub fn new(dpf: D, field: Field) -> Self {
        FieldVDPF { dpf, field }
    }
}

// Pass through DPF methods
impl<D: DPF> DPF for FieldVDPF<D> {
    type Key = D::Key;
    type Message = D::Message;

    fn num_points(&self) -> usize {
        self.dpf.num_points()
    }

    fn num_keys(&self) -> usize {
        self.dpf.num_keys()
    }

    fn null_message(&self) -> Self::Message {
        self.dpf.null_message()
    }

    fn gen(&self, msg: Self::Message, idx: usize) -> Vec<Self::Key> {
        self.dpf.gen(msg, idx)
    }

    fn gen_empty(&self) -> Vec<Self::Key> {
        self.dpf.gen_empty()
    }

    fn eval(&self, key: &Self::Key) -> Vec<Self::Message> {
        self.dpf.eval(key)
    }

    fn combine(&self, parts: Vec<Vec<Self::Message>>) -> Vec<Self::Message> {
        self.dpf.combine(parts)
    }
}

pub type ConcreteVdpf = FieldVDPF<PRGDPF<AESPRG>>;

impl VDPF for ConcreteVdpf {
    type AuthKey = FieldElement;
    type ProofShare = FieldProofShare;
    type Token = FieldToken;

    fn sample_key(&self) -> FieldElement {
        let mut rng = RandState::new();
        self.field.rand_element(&mut rng)
    }

    fn sample_keys(&self) -> Vec<FieldElement> {
        let mut rng = RandState::new();
        repeat_with(|| self.field.rand_element(&mut rng))
            .take(self.num_points())
            .collect()
    }

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
        self.gen_proofs(&self.field.zero(), self.num_points() - 1, dpf_keys)
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

        // TODO: switch to blake3 in parallel when input is ~1 Mbit or greater
        let mut hasher = blake3::Hasher::new();
        hasher.update(dpf_key.encoded_msg.as_ref());
        let data: [u8; 32] = hasher.finalize().into();

        FieldToken {
            bit: bit_check,
            seed: seed_check,
            data: Bytes::from(data.to_vec()),
        }
    }

    fn check_audit(&self, tokens: Vec<FieldToken>) -> bool {
        assert_eq!(tokens.len(), 2, "not implemented");
        tokens[0] == tokens[1]
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use proptest::prelude::*;

    use crate::crypto::{dpf::tests::data_with_dpf, field::tests::integers};

    impl<D: Arbitrary + 'static> Arbitrary for FieldVDPF<D> {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            (any::<D>(), any::<Field>())
                .prop_map(|(dpf, field)| FieldVDPF::new(dpf, field))
                .boxed()
        }
    }

    impl Arbitrary for FieldProofShare {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            (any::<Field>(), integers(), 0..1u8)
                .prop_map(|(field, seed_value, bit)| FieldProofShare {
                    bit: SecretShare::new(field.new_element(bit.into())),
                    seed: SecretShare::new(field.new_element(seed_value)),
                })
                .boxed()
        }
    }

    fn hashes() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(any::<u8>(), 32)
    }

    impl Arbitrary for FieldToken {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            (any::<Field>(), hashes())
                .prop_flat_map(|(field, data)| {
                    (
                        any_with::<FieldElement>(Some(field.clone())),
                        any_with::<FieldElement>(Some(field)),
                    )
                        .prop_map(move |(bit, seed)| FieldToken {
                            bit: SecretShare::new(bit),
                            seed: SecretShare::new(seed),
                            data: data.clone().into(),
                        })
                })
                .boxed()
        }
    }

    fn run_test_audit_check_correct<V: VDPF>(
        vdpf: V,
        auth_keys: &[V::AuthKey],
        data: <V as DPF>::Message,
        point_idx: usize,
    ) {
        let dpf_keys = vdpf.gen(data, point_idx);
        let proof_shares = vdpf.gen_proofs(&auth_keys[point_idx], point_idx, &dpf_keys);
        let audit_tokens = dpf_keys
            .iter()
            .zip(proof_shares.iter())
            .map(|(dpf_key, proof_share)| vdpf.gen_audit(&auth_keys, dpf_key, proof_share))
            .collect();
        assert!(vdpf.check_audit(audit_tokens));
    }

    fn run_test_audit_check_correct_for_noop<V: VDPF>(vdpf: V, auth_keys: &[V::AuthKey]) {
        let dpf_keys = vdpf.gen_empty();
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
            (data, vdpf) in data_with_dpf::<ConcreteVdpf>(),
            point_idx in any::<prop::sample::Index>(),
        ) {
            let auth_keys = vdpf.sample_keys();
            let point_idx = point_idx.index(auth_keys.len());

            run_test_audit_check_correct(vdpf, &auth_keys, data, point_idx);
        }

        #[test]
        fn test_audit_check_correct_for_noop(
            vdpf in any::<ConcreteVdpf>(),
        ) {
            let auth_keys = vdpf.sample_keys();
            run_test_audit_check_correct_for_noop(vdpf, &auth_keys);
        }

    }
}
