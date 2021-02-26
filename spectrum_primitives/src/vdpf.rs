//! Spectrum implementation.
use crate::{
    dpf::{BasicDPF, MultiKeyDPF, DPF},
    field::Field,
    group::GroupElement,
    lss::Shareable,
    prg::aes::AESPRG,
    prg::group::GroupPRG,
    util::Sampleable,
};

use crate::bytes::Bytes;
use rug::Integer;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::fmt::Debug;
use std::iter::repeat_with;
use std::{collections::HashSet, marker::PhantomData};

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

// check_audit(gen_audit(gen_proof(...))) = TRUE
pub trait VDPF: DPF {
    type AuthKey;
    type ProofShare;
    type Token;

    fn new_access_key(&self) -> Self::AuthKey;
    fn new_access_keys(&self) -> Vec<Self::AuthKey>;

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
pub struct FieldToken<F>
where
    F: Shareable,
{
    pub bit: F::Shares,
    pub seed: F::Shares,
    pub data: Bytes,
}

impl<F> FieldToken<F>
where
    F: Shareable,
{
    pub fn new(bit: F::Shares, seed: F::Shares, data: Bytes) -> Self {
        Self { bit, seed, data }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct FieldProofShare<F>
where
    F: Shareable,
{
    pub bit: F::Shares,
    pub seed: F::Shares,
}

impl<F> FieldProofShare<F>
where
    F: Shareable,
{
    pub fn new(bit: F::Shares, seed: F::Shares) -> Self {
        Self { bit, seed }
    }

    fn share(bit_proof: F, seed_proof: F, len: usize) -> Vec<FieldProofShare<F>> {
        let bits = bit_proof.share(len);
        let seeds = seed_proof.share(len);
        bits.into_iter()
            .zip(seeds.into_iter())
            .map(|(bit, seed)| FieldProofShare::new(bit, seed))
            .collect()
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct FieldVDPF<D, F> {
    dpf: D,
    phantom: PhantomData<F>,
}

impl<D, F> FieldVDPF<D, F> {
    pub fn new(dpf: D) -> Self {
        FieldVDPF {
            dpf,
            phantom: Default::default(),
        }
    }
}

// Pass through DPF methods
impl<D: DPF, F> DPF for FieldVDPF<D, F> {
    type Key = D::Key;
    type Message = D::Message;

    fn num_points(&self) -> usize {
        self.dpf.num_points()
    }

    fn num_keys(&self) -> usize {
        self.dpf.num_keys()
    }

    fn msg_size(&self) -> usize {
        self.dpf.msg_size()
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

    fn eval(&self, key: Self::Key) -> Vec<Self::Message> {
        self.dpf.eval(key)
    }

    fn combine(&self, parts: Vec<Vec<Self::Message>>) -> Vec<Self::Message> {
        self.dpf.combine(parts)
    }
}

// TODO(sss) make this more abstract? Don't think that we need both MultiKeyVDPF and BasicVDPF
// should be able to just use abstract DPF notion + properties on PRG seeds (addition)
pub type BasicVdpf = FieldVDPF<BasicDPF<AESPRG>, GroupElement>;
pub type MultiKeyVdpf = FieldVDPF<MultiKeyDPF<GroupPRG<GroupElement>>, GroupElement>;

pub mod two_key {
    use crate::prg::aes::AESSeed;

    use super::*;

    impl<F> VDPF for FieldVDPF<BasicDPF<AESPRG>, F>
    where
        F: Field + Sampleable + Clone + TryFrom<Bytes> + Shareable,
        <F as TryFrom<Bytes>>::Error: Debug,
        F::Shares: From<F> + Clone + Field,
    {
        type AuthKey = F;
        type ProofShare = FieldProofShare<F>;
        type Token = FieldToken<F>;

        fn new_access_key(&self) -> F {
            F::rand_element()
        }

        fn new_access_keys(&self) -> Vec<F> {
            repeat_with(F::rand_element)
                .take(self.num_points())
                .collect()
        }

        fn gen_proofs(
            &self,
            auth_key: &F,
            point_idx: usize,
            dpf_keys: &[<Self as DPF>::Key],
        ) -> Vec<FieldProofShare<F>> {
            assert_eq!(dpf_keys.len(), 2, "not implemented");

            let dpf_key_a = dpf_keys[0].clone();
            let dpf_key_b = dpf_keys[1].clone();

            // 1) generate the proof using the DPF keys and the channel key
            let res_seed = dpf_key_a
                .seeds
                .iter()
                .cloned()
                .map(AESSeed::bytes)
                .map(F::try_from)
                .map(Result::unwrap)
                .fold_first(|x, y| x.add(&y))
                .expect("Seed vector should be nonempty");
            let res_seed = dpf_key_b
                .seeds
                .iter()
                .cloned()
                .map(AESSeed::bytes)
                .map(F::try_from)
                .map(Result::unwrap)
                .fold(res_seed, |x, y| x.add(&y));

            let seed_proof = auth_key.mul(&res_seed);

            // If the bit is `1` for server A, then we need to negate the share
            // to ensure that (bit_A - bit_B = 1)*key = -key and not key
            let bit_proof = if dpf_key_a.bits[point_idx] == 1 {
                auth_key.neg()
            } else {
                auth_key.clone()
            };

            FieldProofShare::share(bit_proof, seed_proof, dpf_keys.len())
        }

        fn gen_proofs_noop(&self, dpf_keys: &[<Self as DPF>::Key]) -> Vec<Self::ProofShare> {
            self.gen_proofs(&F::zero(), self.num_points() - 1, dpf_keys)
        }

        fn gen_audit(
            &self,
            auth_keys: &[F],
            dpf_key: &<Self as DPF>::Key,
            proof_share: &FieldProofShare<F>,
        ) -> FieldToken<F> {
            let mut bit_check = proof_share.bit.clone();
            let mut seed_check = proof_share.seed.clone();
            for ((key, seed), bit) in auth_keys
                .iter()
                .zip(dpf_key.seeds.iter())
                .zip(dpf_key.bits.iter())
            {
                let seed_in_field = F::try_from(seed.clone().bytes()).unwrap();
                seed_check = seed_check.add(&key.clone().mul(&seed_in_field).neg().into());
                match bit {
                    0 => {}
                    1 => bit_check = bit_check.add(&F::Shares::from(key.clone())),
                    _ => panic!("Bit must be 0 or 1"),
                }
            }

            // TODO: switch to blake3 in parallel when input is ~1 Mbit or greater
            let mut hasher = blake3::Hasher::new();
            let input = dpf_key.encoded_msg.as_ref().as_ref();
            if dpf_key.encoded_msg.len() >= 125000 {
                hasher.update_with_join::<blake3::join::RayonJoin>(input);
            } else {
                hasher.update(input);
            }
            let data: [u8; 32] = hasher.finalize().into();

            FieldToken {
                bit: bit_check,
                seed: seed_check,
                data: Bytes::from(data.to_vec()),
            }
        }

        fn check_audit(&self, tokens: Vec<FieldToken<F>>) -> bool {
            assert_eq!(tokens.len(), 2, "not implemented");
            tokens[0] == tokens[1]
        }
    }

    #[cfg(test)]
    pub mod tests {
        use super::super::tests as vdpf_tests;
        use super::*;
        use crate::dpf::two_key as two_key_dpf;

        proptest! {
            #[test]
            fn test_audit_check_correct(
                (data, vdpf) in two_key_dpf::tests::data_with_dpf::<BasicVdpf>(),
                point_idx: prop::sample::Index,
            ) {
                let access_keys = vdpf.new_access_keys();
                let point_idx = point_idx.index(access_keys.len());

                vdpf_tests::run_test_audit_check_correct(vdpf, &access_keys, data, point_idx)?;
            }

            #[test]
            fn test_audit_check_correct_for_noop(vdpf: BasicVdpf) {
                let access_keys = vdpf.new_access_keys();
                vdpf_tests::run_test_audit_check_correct_for_noop(vdpf, &access_keys)?;
            }

        }
    }
}

pub mod multi_key {
    use crate::algebra::Group;

    use super::*;

    impl<F> VDPF for FieldVDPF<MultiKeyDPF<GroupPRG<F>>, F>
    where
        F: Field + Shareable + Clone + Group + Debug + Into<Bytes> + Sampleable + From<Integer>,
        F::Shares: Clone + Field + From<F>,
    {
        type AuthKey = F;
        type ProofShare = FieldProofShare<F>;
        type Token = FieldToken<F>;

        /// samples a new access key for an index
        fn new_access_key(&self) -> F {
            F::rand_element()
        }

        /// samples a new set of access keys for range of indices
        fn new_access_keys(&self) -> Vec<F> {
            repeat_with(F::rand_element)
                .take(self.num_points())
                .collect()
        }

        fn gen_proofs(
            &self,
            access_key: &F,
            point_idx: usize,
            dpf_keys: &[<Self as DPF>::Key],
        ) -> Vec<FieldProofShare<F>> {
            // sum together the seeds at [point_idx]
            // which *should be* the only non-zero coordinate in the DPF eval
            let mut seed_proof = F::zero();
            for key in dpf_keys.iter() {
                seed_proof =
                    seed_proof.add(&access_key.mul(&F::from(key.seeds[point_idx].clone().value())));
            }

            let mut bit_proof = F::zero();
            for key in dpf_keys.iter() {
                match key.bits[point_idx] {
                    0 => {}
                    1 => bit_proof = bit_proof.add(access_key),
                    _ => panic!("Bit must be 0 or 1"),
                }
            }

            FieldProofShare::share(bit_proof, seed_proof, dpf_keys.len())
        }

        fn gen_proofs_noop(&self, dpf_keys: &[<Self as DPF>::Key]) -> Vec<Self::ProofShare> {
            // setting the desired index to self.nulpoints() - 1 results in a noop
            // here access key = 0
            self.gen_proofs(&F::zero(), self.num_points() - 1, dpf_keys)
        }

        fn gen_audit(
            &self,
            access_keys: &[F],
            dpf_key: &<Self as DPF>::Key,
            proof_share: &FieldProofShare<F>,
        ) -> FieldToken<F> {
            let mut bit_check = proof_share.bit.clone();
            let mut seed_check = proof_share.seed.clone();

            let is_first = false;

            for ((access_key, seed), bit) in access_keys
                .iter()
                .zip(dpf_key.seeds.iter())
                .zip(dpf_key.bits.iter())
            {
                if is_first {
                    seed_check = seed_check.add(&F::Shares::from(
                        access_key.mul(&F::from(seed.clone().value().clone()).neg()),
                    ));
                } else {
                    seed_check = seed_check.add(&F::Shares::from(
                        access_key.mul(&F::from(seed.clone().value().clone())),
                    ));
                }

                if is_first {
                    match *bit {
                        0 => {}
                        1 => bit_check = bit_check.add(&F::Shares::from(access_key.clone())),
                        _ => {
                            panic!("Bit must be 0 or 1");
                        }
                    }
                } else {
                    match *bit {
                        0 => {}
                        1 => bit_check = bit_check.add(&F::Shares::from(access_key.clone()).neg()),
                        _ => {
                            panic!("Bit must be 0 or 1");
                        }
                    }
                }
            }

            // TODO: kill this clone
            let msg_hash: Bytes = dpf_key.encoded_msg.clone().hash_all();
            FieldToken {
                bit: bit_check,
                seed: seed_check,
                data: msg_hash,
            }
        }

        fn check_audit(&self, tokens: Vec<FieldToken<F>>) -> bool {
            let bit_proof = F::recover(tokens.iter().map(|t| t.bit.clone()).collect());
            if bit_proof != F::zero() {
                return false;
            }

            let seed_proof = F::recover(tokens.iter().map(|t| t.seed.clone()).collect());
            if seed_proof != F::zero() {
                return false;
            }

            // make sure all hashes are equal
            let distinct_hashes: HashSet<_> = tokens.into_iter().map(|t| t.data).collect();
            if distinct_hashes.len() != 1 {
                return false;
            }

            true
        }
    }

    #[cfg(test)]
    pub mod tests {
        use super::super::tests as vdpf_tests;
        use super::*;
        use crate::dpf::multi_key as multi_key_dpf;

        proptest! {
            #[test]
            fn test_audit_check_correct(
                (data, vdpf) in multi_key_dpf::tests::data_with_dpf::<MultiKeyVdpf, GroupElement>(),
                point_idx: prop::sample::Index,
            ) {
                let access_keys = vdpf.new_access_keys();
                let point_idx = point_idx.index(access_keys.len());

                vdpf_tests::run_test_audit_check_correct(vdpf, &access_keys, data, point_idx)?;
            }

            #[test]
            fn test_audit_check_correct_for_noop(vdpf: MultiKeyVdpf) {
                let access_keys = vdpf.new_access_keys();
                vdpf_tests::run_test_audit_check_correct_for_noop(vdpf, &access_keys)?;
            }

        }
    }
}

#[cfg(any(test, feature = "testing"))]
impl<D, F> Arbitrary for FieldVDPF<D, F>
where
    F: Debug,
    D: Arbitrary + 'static,
{
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        any::<D>()
            .prop_map(|dpf| FieldVDPF::<D, F>::new(dpf))
            .boxed()
    }
}

#[cfg(any(test, feature = "testing"))]
impl<F> Arbitrary for FieldProofShare<F>
where
    F: Debug + Clone + From<Integer> + Shareable,
    F::Shares: Debug + From<F>,
{
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use crate::field::testing::integers;

        (integers(), 0..1u8)
            .prop_map(|(seed_value, bit)| FieldProofShare {
                bit: F::Shares::from(F::from(bit.into())),
                seed: F::Shares::from(F::from(seed_value)),
            })
            .boxed()
    }
}

#[cfg(any(test, feature = "testing"))]
mod testing {
    use proptest::prelude::*;

    pub(super) fn hashes() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(super::any::<u8>(), 32)
    }
}

#[cfg(any(test, feature = "testing"))]
impl<F> Arbitrary for FieldToken<F>
where
    F: Debug + Arbitrary + Shareable + 'static,
    F::Shares: From<F> + Debug + 'static,
{
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use testing::hashes;

        (hashes(), any::<F>(), any::<F>())
            .prop_map(|(data, bit, seed)| FieldToken {
                bit: F::Shares::from(bit),
                seed: F::Shares::from(seed),
                data: data.clone().into(),
            })
            .boxed()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub(super) fn run_test_audit_check_correct<V: VDPF>(
        vdpf: V,
        auth_keys: &[V::AuthKey],
        data: <V as DPF>::Message,
        point_idx: usize,
    ) -> Result<(), TestCaseError> {
        let dpf_keys = vdpf.gen(data, point_idx);
        let proof_shares = vdpf.gen_proofs(&auth_keys[point_idx], point_idx, &dpf_keys);
        let audit_tokens = dpf_keys
            .iter()
            .zip(proof_shares.iter())
            .map(|(dpf_key, proof_share)| vdpf.gen_audit(&auth_keys, dpf_key, proof_share))
            .collect();
        prop_assert!(vdpf.check_audit(audit_tokens));
        Ok(())
    }

    pub(super) fn run_test_audit_check_correct_for_noop<V: VDPF>(
        vdpf: V,
        auth_keys: &[V::AuthKey],
    ) -> Result<(), TestCaseError> {
        let dpf_keys = vdpf.gen_empty();
        let proof_shares = vdpf.gen_proofs_noop(&dpf_keys);
        let audit_tokens = dpf_keys
            .iter()
            .zip(proof_shares.iter())
            .map(|(dpf_key, proof_share)| vdpf.gen_audit(&auth_keys, dpf_key, proof_share))
            .collect();
        prop_assert!(vdpf.check_audit(audit_tokens));
        Ok(())
    }
}
