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
    fn combine_outputs(&self, outputs: Vec<Self::Output>) -> Self::Output;
}

/// Seed homomorphic PRG
pub trait SeedHomomorphicPRG: PRG {
    fn combine_seeds(&self, seeds: Vec<Self::Seed>) -> Self::Seed;
}

/// PRG uses AES to expand a seed to desired length
#[derive(Default, Clone, PartialEq, Debug, Copy, Serialize, Deserialize)]
pub struct AESPRG {
    seed_size: usize,
    eval_size: usize,
}

/// seed for AES-based PRG
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct AESSeed {
    bytes: Bytes,
}

/// evaluation type for AES-based PRG
impl AESSeed {
    pub fn to_field_element(&self, field: Field) -> FieldElement {
        field.element_from_bytes(&self.bytes)
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
        let mut seed_bytes = vec![0; self.seed_size];
        thread_rng().fill_bytes(&mut seed_bytes);

        AESSeed {
            bytes: Bytes::from(seed_bytes),
        }
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

    fn combine_outputs(&self, outputs: Vec<Bytes>) -> Bytes {
        let mut comb = self.null_output();
        for val in outputs.iter() {
            comb ^= val;
        }
        comb
    }
}

// Implementation of a group-based PRG
#[derive(Clone, PartialEq, Debug)]
pub struct GroupPRG {
    generators: Vec<GroupElement>,
    eval_size: usize,
}

impl GroupPRG {
    pub fn new(eval_size: usize, generator_seed: [u8; 16]) -> Self {
        let generators = GroupPRG::get_generators(eval_size, &generator_seed);
        GroupPRG {
            generators,
            eval_size,
        }
    }

    fn get_generators(eval_size: usize, seed: &[u8; 16]) -> Vec<GroupElement> {
        let eval_factor: usize = eval_size / Group::order_byte_size(); // expansion factor (# group elements)
        Group::generators(eval_factor, seed)
    }
}

impl PRG for GroupPRG {
    type Seed = Integer;
    type Output = Vec<GroupElement>;

    /// generates a new (random) seed for the given PRG
    fn new_seed(&self) -> Integer {
        let mut rand_bytes = vec![0; 32];
        thread_rng().fill_bytes(&mut rand_bytes);
        Integer::from_digits(&rand_bytes.as_ref(), Order::LsfLe)
    }

    /// evaluates the PRG on the given seed
    fn eval(&self, seed: &Integer) -> Self::Output {
        self.generators.iter().map(|g| g.pow(seed)).collect()
    }

    fn null_output(&self) -> Self::Output {
        repeat(Group::additive_identity())
            .take(self.generators.len())
            .collect()
    }

    fn combine_outputs(&self, outputs: Vec<Vec<GroupElement>>) -> Vec<GroupElement> {
        let mut comb = self.null_output();
        for output in outputs.iter() {
            for (i, val) in output.iter().enumerate() {
                comb[i] ^= val;
            }
        }
        comb
    }
}

impl SeedHomomorphicPRG for GroupPRG {
    fn combine_seeds(&self, seeds: Vec<Integer>) -> Integer {
        Integer::from(Integer::sum(seeds.iter()))
    }
}

#[cfg(test)]
mod tests {
    extern crate rand;
    use super::*;
    use proptest::prelude::*;
    use rug::Integer;
    use std::collections::HashSet;
    use std::fmt::Debug;
    use std::ops::Range;

    const SIZES: Range<usize> = 16..1000; // in bytes
    const SEED_SIZE: usize = 16; // in bytes
    const GROUP_PRG_EVAL_SIZES: Range<usize> = 64..1000; // in bytes

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

    impl Arbitrary for GroupPRG {
        type Parameters = (usize, [u8; 16]);
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(params: Self::Parameters) -> Self::Strategy {
            Just(GroupPRG::new(params.0, params.1)).boxed()
        }
    }

    pub fn integers() -> impl Strategy<Value = Integer> {
        (0..10).prop_map(Integer::from)
    }
    // group prg and vector of seeds
    pub fn group_prg_and_seed_vec(num: usize) -> impl Strategy<Value = (GroupPRG, Vec<Integer>)> {
        let prg = (GROUP_PRG_EVAL_SIZES, any::<[u8; 16]>()).prop_flat_map(
            move |(output_size, generator_seed)| {
                any_with::<GroupPRG>((output_size, generator_seed))
            },
        );

        // TODO: (fixme) this is too hacky
        let seeds = prg.clone().prop_flat_map(move |prg| {
            prop::collection::vec(integers().prop_map(move |_| prg.new_seed()), num)
        });

        (prg, seeds)
    }

    fn run_test_prg_null_combine<P>(prg: P)
    where
        P: PRG,
        P::Seed: Eq + Debug,
        P::Output: Eq + Debug,
    {
        assert_eq!(
            prg.null_output(),
            prg.combine_outputs(vec![prg.null_output(), prg.null_output()])
        );
    }

    fn run_test_prg_seed_random<P>(prg: P)
    where
        P: PRG,
        P::Seed: Eq + Debug,
        P::Output: Eq + Debug,
    {
        assert_ne!(prg.new_seed(), prg.new_seed());
    }

    fn run_test_prg_eval_deterministic<P>(prg: P, seed: P::Seed)
    where
        P: PRG,
        P::Seed: Eq + Debug,
        P::Output: Eq + Debug,
    {
        assert_eq!(prg.eval(&seed), prg.eval(&seed));
    }

    fn run_test_prg_eval_random<P>(prg: P, seeds: &[P::Seed])
    where
        P: PRG,
        P::Seed: Eq + Debug,
        P::Output: Eq + Debug + Hash,
    {
        let results: HashSet<_> = seeds.iter().map(|s| prg.eval(s)).collect();
        assert_eq!(results.len(), seeds.len());
    }

    fn run_test_prg_eval_homomorphism<P>(prg: P, seeds: Vec<P::Seed>)
    where
        P: SeedHomomorphicPRG,
        P::Seed: Eq + Clone + Debug,
        P::Output: Eq + Clone + Debug + Hash,
    {
        // tests for seed-homomorphism: G(s1) ^ G(s2) = G(s1 * s2)
        let outputs: Vec<P::Output> = seeds.iter().map(|seed| prg.eval(seed)).collect();

        assert_eq!(
            prg.combine_outputs(outputs),
            prg.eval(&prg.combine_seeds(seeds))
        );
    }

    // aes prg testing
    proptest! {
        #[test]
        fn test_aes_prg_seed_random(prg in any::<AESPRG>()) {
            run_test_prg_seed_random(prg);
        }

        #[test]
        fn test_aes_prg_eval_deterministic(
            prg in any::<AESPRG>(),
            seed in any::<AESSeed>()
        ) {
            run_test_prg_eval_deterministic(prg, seed);
        }

        #[test]
        fn test_aes_prg_eval_random(
            prg in any::<AESPRG>(),
            seeds in prop::collection::vec(any::<AESSeed>(), 10),
        ) {
            run_test_prg_eval_random(prg, &seeds);
        }

        #[test]
        fn test_aes_prg_null_combine(prg in any::<AESPRG>()) {
            run_test_prg_null_combine(prg);
        }
    }

    // group prg testing
    proptest! {

        #[test]
        fn test_group_prg_null_combine(prg in any::<GroupPRG>()) {
            run_test_prg_null_combine(prg);
        }

        #[test]
        fn test_group_prg_seed_random(prg in any::<GroupPRG>()) {
            run_test_prg_seed_random(prg);
        }

        #[test]
        fn test_group_prg_eval_deterministic(
            (prg, seeds) in group_prg_and_seed_vec(1)
        ) {
            run_test_prg_eval_deterministic(prg, seeds[0].clone());
        }

        #[test]
        fn test_group_prg_eval_homomorphism(
            (prg, seeds) in group_prg_and_seed_vec(10)
        ) {
            run_test_prg_eval_homomorphism(prg, seeds);
        }

        #[test]
        fn test_group_prg_eval_random(
            (prg, seeds) in group_prg_and_seed_vec(10)
        ) {
            run_test_prg_eval_random(prg, &seeds[..seeds.len()]);
        }
    }
}
