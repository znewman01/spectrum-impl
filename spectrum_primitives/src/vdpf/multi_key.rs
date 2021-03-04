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
