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
