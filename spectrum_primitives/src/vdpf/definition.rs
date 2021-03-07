use crate::dpf::DPF;

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

    fn gen_proofs_noop(&self) -> Vec<Self::ProofShare>;

    fn gen_audit(
        &self,
        auth_keys: &[Self::AuthKey],
        dpf_key: &<Self as DPF>::Key,
        proof_share: Self::ProofShare,
    ) -> Self::Token;

    fn check_audit(&self, tokens: Vec<Self::Token>) -> bool;
}

#[cfg(any(test, feature = "testing"))]
macro_rules! check_vdpf {
    ($type:ty) => {
        #[allow(unused_imports)]
        use crate::{dpf::DPF, vdpf::VDPF};
        #[allow(unused_imports)]
        use proptest::prelude::*;

        #[test]
        fn check_bounds() {
            fn check<V: VDPF>() {}
            check::<$type>();
        }

        fn vdpf_with_keys_data() -> impl Strategy<
            Value = (
                $type,
                Vec<<$type as VDPF>::AuthKey>,
                <$type as DPF>::Message,
            ),
        > {
            any::<$type>().prop_flat_map(|vdpf| {
                (
                    Just(vdpf.clone()),
                    Just(vdpf.new_access_keys()),
                    <$type as DPF>::Message::arbitrary_with(vdpf.msg_size().into()),
                )
            })
        }

        fn vdpf_with_keys() -> impl Strategy<Value = ($type, Vec<<$type as VDPF>::AuthKey>)> {
            any::<$type>().prop_flat_map(|vdpf| (Just(vdpf.clone()), Just(vdpf.new_access_keys())))
        }

        proptest! {
            /// Completeness for gen_proofs.
            #[test]
            fn test_gen_proofs_complete(
                (vdpf, auth_keys, data) in vdpf_with_keys_data(),
                idx: prop::sample::Index
            ) {
                let point_idx = idx.index(vdpf.points());
                let dpf_keys = vdpf.gen(data, point_idx);
                let proof_shares = vdpf.gen_proofs(&auth_keys[point_idx], point_idx, &dpf_keys);
                let audit_tokens = dpf_keys
                    .iter()
                    .zip(proof_shares.into_iter())
                    .map(|(dpf_key, proof_share)| vdpf.gen_audit(&auth_keys, dpf_key, proof_share))
                    .collect();
                prop_assert!(vdpf.check_audit(audit_tokens));
            }

            #[test]
            fn test_gen_proofs_noop_complete((vdpf, auth_keys) in vdpf_with_keys()) {
                let dpf_keys = vdpf.gen_empty();
                let proof_shares = vdpf.gen_proofs_noop();
                let audit_tokens = dpf_keys
                    .iter()
                    .zip(proof_shares.into_iter())
                    .map(|(dpf_key, proof_share)| vdpf.gen_audit(&auth_keys, dpf_key, proof_share))
                    .collect();
                prop_assert!(vdpf.check_audit(audit_tokens));
            }
        }
    };
}
