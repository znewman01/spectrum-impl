//! Spectrum implementation.
use crate::crypto::{
    dpf::{BasicDPF, MultiKeyDPF, DPF},
    field::{Field, FieldElement},
    lss::{SecretShare, LSS},
    prg::aes::AESPRG,
    prg::group::GroupPRG,
};

use crate::bytes::Bytes;
use rug::rand::RandState;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Debug;
use std::iter::repeat_with;

// check_audit(gen_audit(gen_proof(...))) = TRUE
pub trait VDPF: DPF {
    type AuthKey;
    type ProofShare;
    type Token;

    fn sample_access_key(&self) -> Self::AuthKey;
    fn sample_access_keys(&self) -> Vec<Self::AuthKey>;

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

// TODO(sss) make this more abstract? Don't think that we need both MultiKeyVDPF and BasicVDPF
// should be able to just use abstract DPF notion + properties on PRG seeds (addition)
pub type BasicVdpf = FieldVDPF<BasicDPF<AESPRG>>;
pub type MultiKeyVdpf = FieldVDPF<MultiKeyDPF<GroupPRG>>;

pub mod two_key {
    use super::*;

    impl VDPF for BasicVdpf {
        type AuthKey = FieldElement;
        type ProofShare = FieldProofShare;
        type Token = FieldToken;

        fn sample_access_key(&self) -> FieldElement {
            let mut rng = RandState::new();
            self.field.rand_element(&mut rng)
        }

        fn sample_access_keys(&self) -> Vec<FieldElement> {
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

            let seed_proof = auth_key.clone() * res_seed;

            // If the bit is `1` for server A, then we need to negate the share
            // to ensure that (bit_A - bit_B = 1)*key = -key and not key
            let bit_proof = if dpf_key_a.bits[point_idx] == 1 {
                -auth_key.clone()
            } else {
                auth_key.clone()
            };

            FieldProofShare::share(bit_proof, seed_proof, dpf_keys.len())
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
        use super::super::tests as vdpf_tests;
        use super::*;
        use crate::crypto::dpf::two_key as two_key_dpf;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn test_audit_check_correct(
                (data, vdpf) in two_key_dpf::tests::data_with_dpf::<BasicVdpf>(),
                point_idx in any::<prop::sample::Index>(),
            ) {
                let auth_keys = vdpf.sample_access_keys();
                let point_idx = point_idx.index(auth_keys.len());

                vdpf_tests::run_test_audit_check_correct(vdpf, &auth_keys, data, point_idx);
            }

            #[test]
            fn test_audit_check_correct_for_noop(
                vdpf in any::<BasicVdpf>(),
            ) {
                let auth_keys = vdpf.sample_access_keys();
                vdpf_tests::run_test_audit_check_correct_for_noop(vdpf, &auth_keys);
            }

        }
    }
}

pub mod multi_key {
    use super::*;

    impl VDPF for MultiKeyVdpf {
        type AuthKey = FieldElement;
        type ProofShare = FieldProofShare;
        type Token = FieldToken;

        /// samples a new access key for an index
        fn sample_access_key(&self) -> FieldElement {
            let mut rng = RandState::new();
            self.field.rand_element(&mut rng)
        }

        /// samples a new set of access keys for range of indices
        fn sample_access_keys(&self) -> Vec<FieldElement> {
            let mut rng = RandState::new();
            repeat_with(|| self.field.rand_element(&mut rng))
                .take(self.num_points())
                .collect()
        }

        fn gen_proofs(
            &self,
            access_key: &FieldElement,
            point_idx: usize,
            dpf_keys: &[<Self as DPF>::Key],
        ) -> Vec<FieldProofShare> {
            // sum together the seeds at [point_idx]
            // which *should be* the only non-zero coordinate in the DPF eval
            let field = access_key.field();

            let mut seed_proof = field.zero();
            for key in dpf_keys.iter() {
                seed_proof += access_key.clone()
                    * FieldElement::new(key.seeds[point_idx].clone(), field.clone());
            }

            let mut bit_proof = access_key.field().zero();
            for key in dpf_keys.iter() {
                match key.bits[point_idx] {
                    0 => {}
                    1 => bit_proof += access_key.clone(),
                    _ => panic!("Bit must be 0 or 1"),
                }
            }

            FieldProofShare::share(bit_proof, seed_proof, dpf_keys.len())
        }

        fn gen_proofs_noop(&self, dpf_keys: &[<Self as DPF>::Key]) -> Vec<Self::ProofShare> {
            // setting the desired index to self.nulpoints() - 1 results in a noop
            // here access key = 0
            self.gen_proofs(&self.field.zero(), self.num_points() - 1, dpf_keys)
        }

        fn gen_audit(
            &self,
            access_keys: &[FieldElement],
            dpf_key: &<Self as DPF>::Key,
            proof_share: &FieldProofShare,
        ) -> FieldToken {
            let field = access_keys[0].field();
            let mut bit_check = proof_share.bit.clone();
            let mut seed_check = proof_share.seed.clone();

            let is_first = seed_check.clone().is_first();

            for ((access_key, seed), bit) in access_keys
                .iter()
                .zip(dpf_key.seeds.iter())
                .zip(dpf_key.bits.iter())
            {
                if is_first {
                    seed_check -=
                        access_key.clone() * FieldElement::new(seed.clone(), field.clone());
                } else {
                    seed_check +=
                        access_key.clone() * FieldElement::new(seed.clone(), field.clone());
                }

                if is_first {
                    match *bit {
                        0 => {}
                        1 => bit_check -= access_key.clone(),
                        _ => panic!("Bit must be 0 or 1"),
                    }
                } else {
                    match *bit {
                        0 => {}
                        1 => bit_check += access_key.clone(),
                        _ => panic!("Bit must be 0 or 1"),
                    }
                }
            }

            // TODO: (sss) avoid clone
            let msg_bytes: Bytes = dpf_key.encoded_msg.clone().into();

            // TODO: switch to blake3 in parallel when input is ~1 Mbit or greater
            let mut hasher = blake3::Hasher::new();
            hasher.update(msg_bytes.as_ref());
            let data: [u8; 32] = hasher.finalize().into();

            FieldToken {
                bit: bit_check,
                seed: seed_check,
                data: Bytes::from(data.to_vec()),
            }
        }

        fn check_audit(&self, tokens: Vec<FieldToken>) -> bool {
            let bit_proof = LSS::recover(tokens.iter().map(|t| t.bit.clone()).collect());
            let seed_proof = LSS::recover(tokens.iter().map(|t| t.seed.clone()).collect());

            // make sure all hashes are equal ||hash_set|| = 1
            let hash_set: HashSet<_> = tokens.iter().map(|t| t.data.clone()).collect();

            //&& bit_proof.get_value() == 0
            hash_set.len() == 1 && bit_proof.get_value() == 0 && seed_proof.get_value() == 0
        }
    }

    #[cfg(test)]
    pub mod tests {
        use super::super::tests as vdpf_tests;
        use super::*;
        use crate::crypto::dpf::multi_key as multi_key_dpf;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn test_audit_check_correct(
                (data, vdpf) in multi_key_dpf::tests::data_with_dpf::<MultiKeyVdpf>(),
                point_idx in any::<prop::sample::Index>(),
            ) {
                let access_keys = vdpf.sample_access_keys();
                let point_idx = point_idx.index(access_keys.len());

                vdpf_tests::run_test_audit_check_correct(vdpf, &access_keys, data, point_idx);
            }

            #[test]
            fn test_audit_check_correct_for_noop(
                vdpf in any::<MultiKeyVdpf>(),
            ) {
                let access_keys = vdpf.sample_access_keys();
                vdpf_tests::run_test_audit_check_correct_for_noop(vdpf, &access_keys);
            }

        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use proptest::prelude::*;

    use crate::crypto::field::tests::integers;

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
                    bit: SecretShare::new(field.new_element(bit.into()), false),
                    seed: SecretShare::new(field.new_element(seed_value), false),
                })
                .boxed()
        }
    }

    pub(super) fn hashes() -> impl Strategy<Value = Vec<u8>> {
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
                            bit: SecretShare::new(bit, false),
                            seed: SecretShare::new(seed, false),
                            data: data.clone().into(),
                        })
                })
                .boxed()
        }
    }

    pub(super) fn run_test_audit_check_correct<V: VDPF>(
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

    pub(super) fn run_test_audit_check_correct_for_noop<V: VDPF>(
        vdpf: V,
        auth_keys: &[V::AuthKey],
    ) {
        let dpf_keys = vdpf.gen_empty();
        let proof_shares = vdpf.gen_proofs_noop(&dpf_keys);
        let audit_tokens = dpf_keys
            .iter()
            .zip(proof_shares.iter())
            .map(|(dpf_key, proof_share)| vdpf.gen_audit(&auth_keys, dpf_key, proof_share))
            .collect();
        assert!(vdpf.check_audit(audit_tokens));
    }
}
