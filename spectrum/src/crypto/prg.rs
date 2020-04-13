//! Spectrum implementation.
use crate::bytes::Bytes;
use crate::crypto::field::{Field, FieldElement};
use crate::crypto::group::{Group, GroupElement};
use openssl::symm::{encrypt, Cipher};
use rand::prelude::*;
use rug::{integer::Order, Integer};
use serde::{Deserialize, Serialize};
use std::hash::Hash;
use std::iter::repeat;

pub trait PRG {
    type Seed;
    type Output;

    fn new_seed(&self) -> Self::Seed;
    fn eval(&self, seed: &Self::Seed) -> Self::Output;
    fn null_output(&self) -> Self::Output;
}

/// Seed homomorphic PRG
pub trait SeedHomomorphicPRG: PRG {
    fn combine_seeds(&self, seeds: Vec<Self::Seed>) -> Self::Seed;
    fn combine_outputs(&self, outputs: Vec<Self::Output>) -> Self::Output;
}

#[cfg(test)]
mod tests {
    extern crate rand;
    use super::*;
    use std::collections::HashSet;
    use std::fmt::Debug;

    pub fn run_test_prg_null_combine<P>(prg: P, seed: P::Seed)
    where
        P: SeedHomomorphicPRG,
        P::Seed: Eq + Debug,
        P::Output: Eq + Debug,
    {
        // ensure combine(null, null) = null
        assert_eq!(
            prg.null_output(),
            prg.combine_outputs(vec![prg.null_output(), prg.null_output()])
        );

        // ensure combine(null, eval) = eval
        assert_eq!(
            prg.eval(&seed),
            prg.combine_outputs(vec![prg.eval(&seed), prg.null_output()])
        );

        // ensure combine(eval, null) = eval
        assert_eq!(
            prg.eval(&seed),
            prg.combine_outputs(vec![prg.null_output(), prg.eval(&seed)])
        );
    }

    pub fn run_test_prg_seed_random<P>(prg: P)
    where
        P: PRG,
        P::Seed: Eq + Debug,
        P::Output: Eq + Debug,
    {
        assert_ne!(prg.new_seed(), prg.new_seed());
    }

    pub fn run_test_prg_eval_deterministic<P>(prg: P, seed: P::Seed)
    where
        P: PRG,
        P::Seed: Eq + Debug,
        P::Output: Eq + Debug,
    {
        assert_eq!(prg.eval(&seed), prg.eval(&seed));
    }

    pub fn run_test_prg_eval_random<P>(prg: P, seeds: &[P::Seed])
    where
        P: PRG,
        P::Seed: Eq + Debug,
        P::Output: Eq + Debug + Hash,
    {
        let results: HashSet<_> = seeds.iter().map(|s| prg.eval(s)).collect();
        assert_eq!(results.len(), seeds.len());
    }
}

pub mod aes {
    use super::*;

    /// PRG uses AES to expand a seed to desired length
    #[derive(Default, Clone, PartialEq, Debug, Copy, Serialize, Deserialize)]
    pub struct AESPRG {
        seed_size: usize,
        eval_size: usize,
    }

    /// seed for AES-based PRG
    #[derive(Default, Clone, PartialEq, Eq, Debug, Hash)]
    pub struct AESSeed {
        bytes: Bytes,
    }

    /// evaluation type for AES-based PRG
    impl AESSeed {
        pub fn to_field_element(&self, field: Field) -> FieldElement {
            field.element_from_bytes(&self.bytes)
        }

        pub fn random(size: usize) -> Self {
            let mut rand_seed_bytes = vec![0; size];
            thread_rng().fill_bytes(&mut rand_seed_bytes);
            AESSeed::from(rand_seed_bytes)
        }
    }

    impl Into<Vec<u8>> for AESSeed {
        fn into(self) -> Vec<u8> {
            self.bytes.into()
        }
    }

    impl From<Vec<u8>> for AESSeed {
        fn from(other: Vec<u8>) -> Self {
            Self {
                bytes: other.into(),
            }
        }
    }

    impl AESPRG {
        pub fn new(seed_size: usize, eval_size: usize) -> Self {
            assert!(
                seed_size <= eval_size,
                "eval size must be at least the seed size"
            );

            AESPRG {
                seed_size,
                eval_size,
            }
        }
    }

    // Implementation of an AES-based PRG
    impl PRG for AESPRG {
        type Seed = AESSeed;
        type Output = Bytes;

        /// generates a new (random) seed for the given PRG
        fn new_seed(&self) -> AESSeed {
            AESSeed::random(self.seed_size)
        }

        /// evaluates the PRG on the given seed
        fn eval(&self, seed: &AESSeed) -> Self::Output {
            // nonce set to zero: PRG eval should be deterministic
            let iv: [u8; 16] = [0; 16];

            // data is what AES will be "encrypting"
            // must be of size self.eval_size since we want the PRG
            // to expand to that size
            let data = vec![0; self.eval_size];

            // crt mode is fastest and ok for PRG
            let cipher = Cipher::aes_128_ctr();
            let mut ciphertext = encrypt(
                cipher,
                seed.bytes.as_ref(), // use seed bytes as the AES "key"
                Some(&iv),
                &data,
            )
            .unwrap();

            ciphertext.truncate(self.eval_size);
            ciphertext.into()
        }

        fn null_output(&self) -> Bytes {
            Bytes::empty(self.eval_size)
        }
    }

    #[cfg(test)]
    mod tests {
        extern crate rand;
        use super::super::tests as prg_tests;
        use super::*;
        use proptest::prelude::*;
        use std::ops::Range;

        const SIZES: Range<usize> = 16..1000; // in bytes
        const SEED_SIZE: usize = 16; // in bytes

        impl Arbitrary for AESPRG {
            type Parameters = ();
            type Strategy = BoxedStrategy<Self>;

            fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
                SIZES
                    .prop_map(|output_size| AESPRG::new(SEED_SIZE, output_size))
                    .boxed()
            }
        }

        impl Arbitrary for AESSeed {
            type Parameters = ();
            type Strategy = BoxedStrategy<Self>;

            fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
                prop::collection::vec(any::<u8>(), SEED_SIZE)
                    .prop_map(|data| AESSeed { bytes: data.into() })
                    .boxed()
            }
        }

        // aes prg testing
        proptest! {
            #[test]
            fn test_aes_prg_seed_random(prg in any::<AESPRG>()) {
               prg_tests::run_test_prg_seed_random(prg);
            }

            #[test]
            fn test_aes_prg_eval_deterministic(
                prg in any::<AESPRG>(),
                seed in any::<AESSeed>()
            ) {
                prg_tests::run_test_prg_eval_deterministic(prg, seed);
            }

            #[test]
            fn test_aes_prg_eval_random(
                prg in any::<AESPRG>(),
                seeds in prop::collection::vec(any::<AESSeed>(), 10),
            ) {
                prg_tests::run_test_prg_eval_random(prg, &seeds);
            }
        }
    }
}

pub mod group {
    use super::aes::AESSeed;
    use super::*;
    use std::ops::{BitXor, BitXorAssign};

    #[derive(Clone, PartialEq, Eq, Hash, Debug)]
    pub struct ElementVector(pub Vec<GroupElement>);

    // Implementation of a group-based PRG
    #[derive(Clone, PartialEq, Debug)]
    pub struct GroupPRG {
        generators: ElementVector,
        eval_size: usize,
    }

    impl GroupPRG {
        pub fn new(eval_size: usize, generator_seed: AESSeed) -> Self {
            let generators = GroupPRG::compute_generators(eval_size, &generator_seed);
            GroupPRG {
                generators,
                eval_size,
            }
        }

        fn compute_generators(eval_size: usize, seed: &AESSeed) -> ElementVector {
            let num_elements: usize = eval_size / Group::order_size_in_bytes(); // prg eval size (# group elements needed)
            ElementVector(Group::generators(num_elements, seed))
        }
    }

    impl PRG for GroupPRG {
        type Seed = Integer;
        type Output = ElementVector;

        /// generates a new (random) seed for the given PRG
        fn new_seed(&self) -> Integer {
            let mut rand_bytes = vec![0; Group::order_size_in_bytes()];
            thread_rng().fill_bytes(&mut rand_bytes);
            Integer::from_digits(&rand_bytes.as_ref(), Order::LsfLe)
        }

        /// evaluates the PRG on the given seed
        fn eval(&self, seed: &Integer) -> Self::Output {
            ElementVector(self.generators.0.iter().map(|g| g.pow(seed)).collect())
        }

        fn null_output(&self) -> Self::Output {
            ElementVector(
                repeat(Group::identity())
                    .take(self.generators.0.len())
                    .collect(),
            )
        }
    }

    impl SeedHomomorphicPRG for GroupPRG {
        fn combine_seeds(&self, seeds: Vec<Integer>) -> Integer {
            Integer::from(Integer::sum(seeds.iter()))
        }

        fn combine_outputs(&self, outputs: Vec<ElementVector>) -> ElementVector {
            let mut combined = self.null_output();

            for output in outputs {
                for (acc, val) in combined.0.iter_mut().zip(output.0.iter()) {
                    *acc *= val
                }
            }
            combined
        }
    }

    impl BitXor<ElementVector> for ElementVector {
        type Output = ElementVector;

        // TODO(sss): any way around the clone here?
        // apply the group operation on each component in the vector
        fn bitxor(self, rhs: ElementVector) -> ElementVector {
            ElementVector(
                self.0
                    .iter()
                    .zip(rhs.0.iter())
                    .map(|(element1, element2)| element1 * element2)
                    .collect(),
            )
        }
    }

    impl BitXorAssign<ElementVector> for ElementVector {
        // TODO(sss): don't make a totally new elementvector here?
        // apply the group operation on each component in the vector
        fn bitxor_assign(&mut self, rhs: ElementVector) {
            self.0 = self
                .0
                .iter()
                .zip(rhs.0.iter())
                .map(|(element1, element2)| element1 * element2)
                .collect();
        }
    }

    #[cfg(test)]
    mod tests {
        extern crate rand;
        use super::super::tests as prg_tests;
        use super::aes::AESSeed;
        use super::*;
        use proptest::prelude::*;
        use rug::Integer;
        use std::collections::HashSet;
        use std::fmt::Debug;
        use std::ops::Range;

        const GROUP_PRG_EVAL_SIZES: Range<usize> = 64..1000; // in bytes

        impl Arbitrary for GroupPRG {
            type Parameters = Option<(usize, AESSeed)>;
            type Strategy = BoxedStrategy<Self>;

            fn arbitrary_with(params: Self::Parameters) -> Self::Strategy {
                match params {
                    Some(params) => Just(GroupPRG::new(params.0, params.1)).boxed(),
                    None => (GROUP_PRG_EVAL_SIZES, any::<AESSeed>())
                        .prop_flat_map(move |(output_size, generator_seed)| {
                            Just(GroupPRG::new(output_size, generator_seed))
                        })
                        .boxed(),
                }
            }
        }

        fn seed() -> impl Strategy<Value = Integer> {
            (0..1000).prop_map(Integer::from)
        }

        pub fn seeds() -> impl Strategy<Value = Vec<Integer>> {
            prop::collection::vec(seed(), 1..100)
        }

        /// tests for seed-homomorphism: G(s1) ^ G(s2) = G(s1 * s2)
        fn run_test_prg_eval_homomorphism<P>(prg: P, seeds: Vec<P::Seed>)
        where
            P: SeedHomomorphicPRG,
            P::Seed: Eq + Clone + Debug,
            P::Output: Eq + Clone + Debug + Hash,
        {
            let outputs: Vec<P::Output> = seeds.iter().map(|seed| prg.eval(seed)).collect();

            assert_eq!(
                prg.combine_outputs(outputs),
                prg.eval(&prg.combine_seeds(seeds))
            );
        }

        // group prg testing
        proptest! {
            #[test]
            fn test_group_prg_seed_random(prg: GroupPRG) {
                prg_tests::run_test_prg_seed_random(prg);
            }

            #[test]
            fn test_group_prg_eval_deterministic(prg: GroupPRG, seed in seed())
             {
                prg_tests::run_test_prg_eval_deterministic(prg, seed);
            }

            #[test]
            fn test_group_prg_eval_random(prg: GroupPRG, seeds in seeds())
             {
                let unique: HashSet<_> = seeds.iter().cloned().collect();
                prop_assume!(unique.len() == seeds.len(), "eval must be different for different seeds");
                prg_tests::run_test_prg_eval_random(prg, &seeds);
            }
        }

        // seed homomorphic prg tests
        proptest! {
            #[test]
            fn test_group_prg_eval_homomorphism(
                prg: GroupPRG, seeds in seeds(),
            ) {
                run_test_prg_eval_homomorphism(prg, seeds);
            }


            #[test]
            fn test_group_prg_null_combine(prg: GroupPRG, seed in seed()) {
                prg_tests::run_test_prg_null_combine(prg, seed);
            }
        }
    }
}
