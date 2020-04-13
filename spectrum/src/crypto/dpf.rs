//! Spectrum implementation.
#![allow(clippy::unknown_clippy_lints)] // below issue triggers only on clippy beta/nightly
#![allow(clippy::match_single_binding)] // https://github.com/mcarton/rust-derivative/issues/58

use std::fmt::Debug;

/// Distributed Point Function
/// Must generate a set of keys k_1, k_2, ...
/// such that combine(eval(k_1), eval(k_2), ...) = e_i * msg
pub trait DPF {
    type Key;
    type Message;

    fn num_points(&self) -> usize;
    fn num_keys(&self) -> usize;
    fn null_message(&self) -> Self::Message;

    /// Generate `num_keys` DPF keys, the results of which differ only at the given index.
    fn gen(&self, msg: Self::Message, idx: usize) -> Vec<Self::Key>;
    fn gen_empty(&self) -> Vec<Self::Key>;
    fn eval(&self, key: &Self::Key) -> Vec<Self::Message>;
    fn combine(&self, parts: Vec<Vec<Self::Message>>) -> Vec<Self::Message>;
}

pub type BasicDPF<P> = two_server_dpf::Construction<P>;
pub type MultiKeyDPF<P> = multi_key_dpf::Construction<P>;

// 2-DPF (i.e. num_keys = 2) based on any PRG G(.).
mod two_server_dpf {
    use super::*;
    use crate::crypto::prg::PRG;

    use derivative::Derivative;
    use rand::{thread_rng, Rng};
    use serde::{Deserialize, Serialize};

    use std::iter::repeat_with;
    use std::ops;

    #[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
    pub struct Construction<P> {
        prg: P,
        num_points: usize,
    }

    impl<P> Construction<P> {
        pub fn new(prg: P, num_points: usize) -> Construction<P> {
            Construction { prg, num_points }
        }
    }

    #[derive(Derivative)]
    #[derivative(
        Debug(bound = "P::Seed: Debug, P::Output: Debug"),
        PartialEq(bound = "P::Seed: PartialEq, P::Output: PartialEq"),
        Eq(bound = "P::Seed: Eq, P::Output: Eq"),
        Clone(bound = "P::Seed: Clone, P::Output: Clone")
    )]
    pub struct Key<P: PRG> {
        pub encoded_msg: P::Output,
        pub bits: Vec<u8>,
        pub seeds: Vec<<P as PRG>::Seed>,
    }

    impl<P: PRG> Key<P> {
        // generates a new DPF key with the necessary parameters needed for evaluation
        pub fn new(encoded_msg: P::Output, bits: Vec<u8>, seeds: Vec<P::Seed>) -> Key<P> {
            assert_eq!(bits.len(), seeds.len());
            assert!(
                bits.iter().all(|b| *b == 0 || *b == 1),
                "All bits must be 0 or 1"
            );
            Key {
                encoded_msg,
                bits,
                seeds,
            }
        }
    }

    impl<P> DPF for Construction<P>
    where
        P: PRG + Clone,
        P::Seed: Clone + PartialEq + Eq + Debug,
        P::Output: Clone
            + PartialEq
            + Eq
            + Debug
            + ops::BitXor<P::Output, Output = P::Output>
            + ops::BitXorAssign<P::Output>,
    {
        type Key = Key<P>;
        type Message = P::Output;

        fn num_points(&self) -> usize {
            self.num_points
        }

        fn num_keys(&self) -> usize {
            2 // this construction only works for s = 2
        }

        fn null_message(&self) -> Self::Message {
            self.prg.null_output()
        }

        /// generate new instance of PRG based DPF with two DPF keys
        fn gen(&self, msg: Self::Message, point_idx: usize) -> Vec<Key<P>> {
            let seeds_a: Vec<_> = repeat_with(|| self.prg.new_seed())
                .take(self.num_points)
                .collect();
            let bits_a: Vec<_> = repeat_with(|| thread_rng().gen_range(0, 2))
                .take(self.num_points)
                .collect();

            let mut seeds_b = seeds_a.clone();
            seeds_b[point_idx] = self.prg.new_seed();

            let mut bits_b = bits_a.clone();
            bits_b[point_idx] = 1 - bits_b[point_idx];

            let encoded_msg =
                self.prg.eval(&seeds_a[point_idx]) ^ self.prg.eval(&seeds_b[point_idx]) ^ msg;

            vec![
                Key::new(encoded_msg.clone(), bits_a, seeds_a),
                Key::new(encoded_msg, bits_b, seeds_b),
            ]
        }

        fn gen_empty(&self) -> Vec<Key<P>> {
            let seeds: Vec<_> = repeat_with(|| self.prg.new_seed())
                .take(self.num_points)
                .collect();
            let bits: Vec<_> = repeat_with(|| thread_rng().gen_range(0, 2))
                .take(self.num_points)
                .collect();
            let encoded_msg = self.prg.eval(&self.prg.new_seed()); // random message

            vec![Key::new(encoded_msg, bits, seeds); 2]
        }

        /// evaluates the DPF on a given PRGKey and outputs the resulting data
        fn eval(&self, key: &Key<P>) -> Vec<P::Output> {
            key.seeds
                .iter()
                .zip(key.bits.iter())
                .map(|(seed, bits)| {
                    let mut data = self.prg.eval(seed);
                    if *bits == 1 {
                        // TODO(zjn): futz with lifetimes; remove clone()
                        data ^= key.encoded_msg.clone();
                    }
                    data
                })
                .collect()
        }

        /// combines the results produced by running eval on both keys
        fn combine(&self, parts: Vec<Vec<P::Output>>) -> Vec<P::Output> {
            let mut parts = parts.into_iter();
            let mut res = parts.next().expect("Need at least one part to combine.");
            for part in parts {
                for (x, y) in res.iter_mut().zip(part.into_iter()) {
                    *x ^= y;
                }
            }
            res
        }
    }

    #[cfg(test)]
    mod two_server_dpf_tests {
        use super::*;
        use crate::crypto::dpf::prg_tests::*;
        use crate::crypto::prg::{aes::AESPRG, PRG};
        use proptest::prelude::*;

        const MAX_NUM_POINTS: usize = 10;

        impl<P: Arbitrary + 'static> Arbitrary for Construction<P> {
            type Parameters = ();
            type Strategy = BoxedStrategy<Self>;

            fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
                (any::<P>(), 1..=MAX_NUM_POINTS)
                    .prop_map(move |(prg, num_points)| Construction::new(prg, num_points))
                    .boxed()
            }
        }

        impl<P> Arbitrary for Key<P>
        where
            P: PRG,
            P::Seed: Arbitrary,
            P::Output: Arbitrary,
        {
            type Parameters = ();
            type Strategy = BoxedStrategy<Self>;

            fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
                (1..10usize)
                    .prop_flat_map(|num_keys| {
                        (
                            any::<P::Output>(),
                            prop::collection::vec(0..1u8, num_keys),
                            prop::collection::vec(any::<P::Seed>(), num_keys),
                        )
                            .prop_map(|(msg, bits, seeds)| Self::new(msg, bits, seeds))
                    })
                    .boxed()
            }
        }

        proptest! {
            #[test]
            fn test_prg_dpf(
                (data, dpf) in data_with_dpf::<BasicDPF<AESPRG>>(),
                index in any::<proptest::sample::Index>(),
            ) {
                let index = index.index(dpf.num_points());
                run_test_dpf(dpf, data, index);
            }

            #[test]
            fn test_prg_dpf_empty(
                dpf in any::<BasicDPF<AESPRG>>(),
            ) {
                run_test_dpf_empty(dpf);
            }
        }
    }
}

// s-DPF (i.e. num_keys = s > 2) based on any seed-homomorphic PRG G(.).
mod multi_key_dpf {
    use super::*;
    use crate::crypto::prg::SeedHomomorphicPRG;
    use crate::crypto::prg::PRG;
    use derivative::Derivative;
    use serde::{Deserialize, Serialize};
    use std::iter::repeat_with;
    use std::ops;

    #[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
    pub struct Construction<P> {
        prg: P,
        num_points: usize,
        num_keys: usize,
    }

    impl<P> Construction<P> {
        pub fn new(prg: P, num_points: usize, num_keys: usize) -> Construction<P> {
            Construction {
                prg,
                num_points,
                num_keys,
            }
        }
    }

    #[derive(Derivative)]
    #[derivative(
        Debug(bound = "P::Seed: Debug, P::Output: Debug"),
        PartialEq(bound = "P::Seed: PartialEq, P::Output: PartialEq"),
        Eq(bound = "P::Seed: Eq, P::Output: Eq"),
        Clone(bound = "P::Seed: Clone, P::Output: Clone")
    )]
    pub struct Key<P: SeedHomomorphicPRG> {
        pub encoded_msg: P::Output,
        pub bits: Vec<u8>,
        pub seeds: Vec<<P as PRG>::Seed>,
    }

    impl<P: SeedHomomorphicPRG> Key<P> {
        // generates a new DPF key with the necessary parameters needed for evaluation
        pub fn new(encoded_msg: P::Output, bits: Vec<u8>, seeds: Vec<P::Seed>) -> Key<P> {
            assert_eq!(bits.len(), seeds.len());
            assert!(
                bits.iter().all(|b| *b == 0 || *b == 1),
                "All bits must be 0 or 1"
            );
            Key {
                encoded_msg,
                bits,
                seeds,
            }
        }
    }

    impl<P> DPF for Construction<P>
    where
        P: PRG + Clone + SeedHomomorphicPRG,
        P::Seed: Clone
            + PartialEq
            + Eq
            + Debug
            + ops::Sub<Output = P::Seed>
            + ops::Add
            + ops::SubAssign
            + ops::AddAssign,
        P::Output: Clone + PartialEq + Eq + Debug,
    {
        type Key = Key<P>;
        type Message = P::Output;

        fn num_points(&self) -> usize {
            self.num_points
        }

        fn num_keys(&self) -> usize {
            self.num_keys
        }

        fn null_message(&self) -> Self::Message {
            self.prg.null_output()
        }

        /// generate new instance of PRG based DPF with two DPF keys
        fn gen(&self, msg: Self::Message, point_idx: usize) -> Vec<Key<P>> {
            let mut keys = self.gen_empty();

            // generate a new random seed for the specified index
            let special_seed = self.prg.new_seed();

            keys[0].seeds[point_idx] += special_seed.clone();
            keys[0].bits[point_idx] ^= 1;

            // add message to the set of PRG outputs to "combine" together in the next step
            // encoded message G(S*) ^ msg
            let neg = self.prg.null_seed() - special_seed;
            let encoded_msg = self.prg.combine_outputs(vec![msg, self.prg.eval(&neg)]);

            // update the encoded message in the keys
            for key in keys.iter_mut() {
                key.encoded_msg = encoded_msg.clone();
            }

            keys
        }

        fn gen_empty(&self) -> Vec<Key<P>> {
            // vector of seeds for each key
            let mut seeds: Vec<Vec<_>> = repeat_with(|| {
                repeat_with(|| self.prg.new_seed())
                    .take(self.num_points)
                    .collect()
            })
            .take(self.num_keys - 1)
            .collect();

            // want all seeds to cancel out; set last seed to be negation of all former seeds
            let mut last_seed_vec = vec![self.prg.null_seed(); self.num_points];
            for seed_vec in seeds.iter() {
                for (a, b) in last_seed_vec.iter_mut().zip(seed_vec.iter()) {
                    *a -= b.clone()
                }
            }
            seeds.push(last_seed_vec);

            // vector of bits for each key
            let mut bits: Vec<Vec<_>> =
                repeat_with(|| repeat_with(|| 0).take(self.num_points).collect())
                    .take(self.num_keys)
                    .collect();

            // want bits to cancel out; set the last bit to be the xor of all the other bits
            let mut last_bit_vec = vec![0; self.num_points];
            for bit_vec in bits.iter() {
                for (a, b) in last_bit_vec.iter_mut().zip(bit_vec.iter()) {
                    *a ^= b
                }
            }

            bits.push(last_bit_vec);

            let encoded_msg = self.prg.eval(&self.prg.new_seed()); // psuedo random message

            seeds
                .iter()
                .zip(bits.iter())
                .map(|(seed_vec, bit_vec)| {
                    Key::new(encoded_msg.clone(), bit_vec.clone(), seed_vec.clone())
                })
                .collect()
        }

        /// evaluates the DPF on a given PRGKey and outputs the resulting data
        fn eval(&self, key: &Key<P>) -> Vec<P::Output> {
            key.seeds
                .iter()
                .zip(key.bits.iter())
                .map(|(seed, bits)| {
                    let mut data = self.prg.eval(seed);
                    if *bits == 1 {
                        // TODO(zjn): futz with lifetimes; remove clone()
                        data = self
                            .prg
                            .combine_outputs(vec![key.encoded_msg.clone(), data]);
                    }
                    data
                })
                .collect()
        }

        /// combines the results produced by running eval on both keys
        fn combine(&self, parts: Vec<Vec<P::Output>>) -> Vec<P::Output> {
            let mut parts = parts.into_iter();
            let mut res = parts.next().expect("Need at least one part to combine.");
            for part in parts {
                for (x, y) in res.iter_mut().zip(part.into_iter()) {
                    *x = self.prg.combine_outputs(vec![x.clone(), y]);
                }
            }
            res
        }
    }
}

#[cfg(test)]
pub mod prg_tests {
    use super::*;
    use crate::bytes::Bytes;
    use proptest::prelude::*;

    pub fn data_with_dpf<D>() -> impl Strategy<Value = (Bytes, D)>
    where
        D: DPF<Message = Bytes> + Arbitrary + Clone,
    {
        any::<D>().prop_flat_map(|dpf| {
            (
                any_with::<Bytes>(dpf.null_message().len().into()),
                Just(dpf),
            )
        })
    }

    pub(super) fn run_test_dpf<D>(dpf: D, data: D::Message, index: usize)
    where
        D: DPF,
        D::Message: Eq + Debug + Default + Clone,
    {
        let dpf_keys = dpf.gen(data.clone(), index);
        let dpf_shares = dpf_keys.iter().map(|k| dpf.eval(k)).collect();
        let dpf_output = dpf.combine(dpf_shares);

        for (chunk_idx, chunk) in dpf_output.into_iter().enumerate() {
            if chunk_idx == index {
                assert_eq!(chunk, data);
            } else {
                assert_eq!(chunk, dpf.null_message());
            }
        }
    }

    pub(super) fn run_test_dpf_empty<D>(dpf: D)
    where
        D: DPF,
        D::Message: Default + Eq + Debug,
    {
        let dpf_keys = dpf.gen_empty();
        let dpf_shares = dpf_keys.iter().map(|k| dpf.eval(k)).collect();
        let dpf_output = dpf.combine(dpf_shares);

        for chunk in dpf_output {
            assert_eq!(chunk, dpf.null_message());
        }
    }
}
