/// Distributed Point Function
/// Must generate a set of keys k_1, k_2, ...
/// such that combine(eval(k_1), eval(k_2), ...) = e_i * msg
pub trait DPF {
    type Key;
    type Message;

    fn points(&self) -> usize;
    fn keys(&self) -> usize;
    fn null_message(&self) -> Self::Message;
    fn msg_size(&self) -> usize;

    /// Generate `keys` DPF keys, the results of which differ only at the given index.
    fn gen(&self, msg: Self::Message, idx: usize) -> Vec<Self::Key>;
    fn gen_empty(&self) -> Vec<Self::Key>;
    fn eval(&self, key: Self::Key) -> Vec<Self::Message>;
    fn combine(&self, parts: Vec<Vec<Self::Message>>) -> Vec<Self::Message>;
}

#[cfg(any(test, feature = "testing"))]
macro_rules! check_dpf {
    ($type:ty,$mod_name:ident) => {
        mod $mod_name {
            #![allow(unused_imports)]
            use super::*;
            use crate::dpf::DPF;
            use proptest::prelude::*;
            use std::collections::HashSet;
            use std::iter::repeat_with;

            #[test]
            fn check_bounds() {
                fn check<D: DPF>() {}
                check::<$type>();
            }

            fn dpf_with_data() -> impl Strategy<Value = ($type, <$type as DPF>::Message)> {
                any::<$type>().prop_flat_map(|dpf| {
                    (
                        Just(dpf.clone()),
                        <$type as DPF>::Message::arbitrary_with(dpf.msg_size().into()),
                    )
                })
            }

            proptest! {
                #[test]
                fn test_correct((dpf, data) in dpf_with_data(), index: prop::sample::Index) {
                    assert_eq!(data.len(), dpf.msg_size());
                    let index = index.index(dpf.points());
                    let dpf_keys = dpf.gen(data.clone(), index);
                    let dpf_shares = dpf_keys.into_iter().map(|k| dpf.eval(k)).collect();
                    let dpf_output = dpf.combine(dpf_shares);

                    for (chunk_idx, chunk) in dpf_output.into_iter().enumerate() {
                        if chunk_idx == index {
                            prop_assert_eq!(chunk, data.clone());
                        } else {
                            prop_assert_eq!(chunk, dpf.null_message());
                        }
                    }
                }

                #[test]
                fn test_correct_empty(dpf: $type) {
                    let dpf_keys = dpf.gen_empty();
                    let dpf_shares = dpf_keys.into_iter().map(|k| dpf.eval(k)).collect();
                    let dpf_output = dpf.combine(dpf_shares);

                    for chunk in dpf_output {
                        prop_assert_eq!(chunk, dpf.null_message());
                    }
                }
            }
        }
    };
    ($type:ty) => {
        check_dpf!($type, dpf);
    };
}
