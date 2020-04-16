//! Spectrum implementation.
use crate::bytes::Bytes;
use crate::crypto::field::{Field, FieldElement};
use crate::crypto::group::{Group, GroupElement};

use openssl::symm::{encrypt, Cipher};
use rand::prelude::*;
use rug::{integer::Order, Integer};
use serde::{Deserialize, Serialize};

use std::convert::TryFrom;
use std::hash::Hash;
use std::iter::repeat;

pub trait PRG {
    type Seed;
    type Output;

    fn new_seed(&self) -> Self::Seed;
    fn output_size(&self) -> usize;
    fn eval(&self, seed: &Self::Seed) -> Self::Output;
    fn null_output(&self) -> Self::Output;
}

/// Seed homomorphic PRG
pub trait SeedHomomorphicPRG: PRG {
    fn combine_seeds(&self, seeds: Vec<Self::Seed>) -> Self::Seed;
    fn combine_outputs(&self, outputs: Vec<Self::Output>) -> Self::Output;
    fn null_seed(&self) -> Self::Seed;
}

#[cfg(test)]
mod tests {
    extern crate rand;
    use super::*;
    use std::collections::HashSet;
    use std::fmt::Debug;
    use std::ops;

    pub fn run_test_prg_null_combine<P>(prg: P, seed: P::Seed)
    where
        P: SeedHomomorphicPRG,
        P::Seed: Eq + Debug,
        P::Output:
            Eq + Debug + ops::BitXor<P::Output, Output = P::Output> + ops::BitXorAssign<P::Output>,
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

        fn output_size(&self) -> usize {
            self.eval_size
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

    use itertools::Itertools;

    use std::ops::{self, BitXor, BitXorAssign};

    #[derive(Default, Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
    pub struct ElementVector(pub Vec<GroupElement>);

    impl ElementVector {
        pub fn hash_all(self) -> Bytes {
            let mut hasher = blake3::Hasher::new();
            for element in self.0 {
                let chunk: Bytes = element.into();
                let chunk: Vec<u8> = chunk.into();
                hasher.update(&chunk);
            }
            let data: [u8; 32] = hasher.finalize().into();
            data.to_vec().into()
        }
    }

    impl From<Bytes> for ElementVector {
        fn from(bytes: Bytes) -> Self {
            // Turns out the group can't represent a lot of 32-byte values
            // because its modulus is < 2^32.
            // We use (very unnatural) 31-byte chunks so that
            // element_from_bytes() succeeds.
            let chunk_size = Group::order_size_in_bytes() - 1;
            ElementVector(
                bytes
                    .into_iter()
                    .chunks(chunk_size)
                    .into_iter()
                    .map(|data| {
                        let mut data: Vec<u8> = data.collect();
                        while data.len() < Group::order_size_in_bytes() {
                            data.push(0);
                        }
                        let data = Bytes::from(data);
                        GroupElement::try_from(data)
                            .expect("chunk size chosen s.t. this never fails")
                    })
                    .collect(),
            )
        }
    }

    // Implementation of a group-based PRG
    #[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
    pub struct GroupPRG {
        generators: ElementVector,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct GroupPrgSeed {
        value: Integer,
    }

    impl GroupPrgSeed {
        pub fn new(value: Integer) -> Self {
            let mut value = value;
            while value < 0 {
                value += Group::order();
            }
            if value >= Group::order() {
                value = (value % Group::order()).into();
            }
            GroupPrgSeed { value }
        }

        pub fn value(self) -> Integer {
            self.value
        }
    }

    impl ops::Sub for GroupPrgSeed {
        type Output = Self;

        fn sub(self, other: Self) -> Self {
            GroupPrgSeed::new(Integer::from(self.value - other.value))
        }
    }

    impl ops::SubAssign for GroupPrgSeed {
        fn sub_assign(&mut self, other: Self) {
            self.value -= other.value;
            if self.value < 0 {
                self.value += Group::order();
            }
        }
    }

    impl ops::Add for GroupPrgSeed {
        type Output = Self;

        fn add(self, other: Self) -> Self {
            GroupPrgSeed::new(Integer::from(self.value + other.value))
        }
    }

    impl ops::AddAssign for GroupPrgSeed {
        fn add_assign(&mut self, other: Self) {
            self.value += other.value;
            if self.value >= Group::order() {
                self.value -= Group::order();
            }
        }
    }

    impl Into<Vec<u8>> for GroupPrgSeed {
        fn into(self) -> Vec<u8> {
            self.value.to_string_radix(10).into_bytes()
        }
    }

    impl From<Vec<u8>> for GroupPrgSeed {
        fn from(data: Vec<u8>) -> Self {
            let data = String::from_utf8(data).unwrap();
            GroupPrgSeed::new(Integer::parse_radix(&data, 10).unwrap().into())
        }
    }

    impl GroupPRG {
        pub fn new(generators: ElementVector) -> Self {
            GroupPRG { generators }
        }

        pub fn from_aes_seed(num_elements: usize, generator_seed: AESSeed) -> Self {
            let generators = GroupPRG::compute_generators(num_elements, &generator_seed);
            GroupPRG::new(generators)
        }

        fn compute_generators(num_elements: usize, seed: &AESSeed) -> ElementVector {
            ElementVector(Group::generators(num_elements, seed))
        }
    }

    impl PRG for GroupPRG {
        type Seed = GroupPrgSeed;
        type Output = ElementVector;

        /// generates a new (random) seed for the given PRG
        fn new_seed(&self) -> Self::Seed {
            let mut rand_bytes = vec![0; Group::order_size_in_bytes()];
            thread_rng().fill_bytes(&mut rand_bytes);
            GroupPrgSeed::new(Integer::from_digits(&rand_bytes.as_ref(), Order::LsfLe))
        }

        /// evaluates the PRG on the given seed
        fn eval(&self, seed: &Self::Seed) -> Self::Output {
            ElementVector(
                self.generators
                    .0
                    .iter()
                    .map(|g| g.pow(&seed.clone().value()))
                    .collect(),
            )
        }

        fn null_output(&self) -> Self::Output {
            ElementVector(
                repeat(Group::identity())
                    .take(self.generators.0.len())
                    .collect(),
            )
        }

        fn output_size(&self) -> usize {
            self.generators.0.len() * (Group::order_size_in_bytes() - 1)
        }
    }

    impl SeedHomomorphicPRG for GroupPRG {
        fn null_seed(&self) -> Self::Seed {
            GroupPrgSeed::new(0.into())
        }

        fn combine_seeds(&self, seeds: Vec<GroupPrgSeed>) -> GroupPrgSeed {
            let seeds: Vec<Integer> = seeds.into_iter().map(|s| s.value()).collect();
            GroupPrgSeed::new(Integer::from(Integer::sum(seeds.iter())) % Group::order())
        }

        fn combine_outputs(&self, outputs: Vec<ElementVector>) -> ElementVector {
            let mut combined = self.null_output();
            for output in outputs {
                for (acc, val) in combined.0.iter_mut().zip(output.0.iter()) {
                    *acc *= val;
                }
            }
            combined
        }
    }

    // TODO: should be try_into()
    impl Into<Bytes> for ElementVector {
        fn into(self) -> Bytes {
            let chunk_size = Group::order_size_in_bytes() - 1;
            // outputs all the elements in the vector concatenated as a sequence of bytes
            // assumes that every element is < 2^(8*31)
            let mut all_bytes = Vec::with_capacity(chunk_size * self.0.len());
            for element in self.0.into_iter() {
                let bytes: Bytes = element.into();
                let bytes: Vec<u8> = bytes.into();
                assert_eq!(bytes.clone()[31], 0);
                let bytes = Bytes::from(bytes[0..31].to_vec());
                all_bytes.append(&mut bytes.into());
            }
            Bytes::from(all_bytes)
        }
    }

    impl BitXor<ElementVector> for ElementVector {
        type Output = ElementVector;

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

    impl Into<Vec<u8>> for ElementVector {
        fn into(self) -> Vec<u8> {
            let chunk_size = Group::order_size_in_bytes();
            // outputs all the elements in the vector concatenated as a sequence of bytes
            // assumes that every element is < 2^(8*31)
            let mut all_bytes = Vec::with_capacity(chunk_size * self.0.len());
            for element in self.0.into_iter() {
                let bytes: Bytes = element.into();
                let bytes: Vec<u8> = bytes.into();
                all_bytes.append(&mut bytes.into());
            }
            all_bytes
        }
    }

    impl From<Vec<u8>> for ElementVector {
        fn from(bytes: Vec<u8>) -> Self {
            let chunk_size = Group::order_size_in_bytes();
            // outputs all the elements in the vector concatenated as a sequence of bytes
            let mut elements = vec![];
            for chunk in bytes.into_iter().chunks(chunk_size).into_iter() {
                elements
                    .push(GroupElement::try_from(Bytes::from(chunk.collect::<Vec<u8>>())).unwrap());
            }
            ElementVector(elements)
        }
    }

    impl BitXorAssign<ElementVector> for ElementVector {
        /// Apply the group operation on each component in the vector.
        // There's a mismatch between operations because we require that the PRG
        // output be XOR-able (and some properties on that).
        #[allow(clippy::suspicious_op_assign_impl)]
        fn bitxor_assign(&mut self, rhs: ElementVector) {
            self.0
                .iter_mut()
                .zip(rhs.0.iter())
                .for_each(|(element1, element2)| *element1 *= element2);
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
        use std::ops;
        use std::ops::Range;

        const GROUP_PRG_NUM_SEEDS: Range<usize> = 1..10; // # group elements

        impl Arbitrary for GroupPRG {
            type Parameters = Option<(usize, AESSeed)>;
            type Strategy = BoxedStrategy<Self>;

            fn arbitrary_with(params: Self::Parameters) -> Self::Strategy {
                match params {
                    Some(params) => Just(GroupPRG::from_aes_seed(params.0, params.1)).boxed(),
                    None => (GROUP_PRG_NUM_SEEDS, any::<AESSeed>())
                        .prop_flat_map(move |(output_size, generator_seed)| {
                            Just(GroupPRG::from_aes_seed(output_size, generator_seed))
                        })
                        .boxed(),
                }
            }
        }

        impl Arbitrary for ElementVector {
            type Parameters = prop::collection::SizeRange;
            type Strategy = BoxedStrategy<Self>;

            fn arbitrary_with(num_elements: Self::Parameters) -> Self::Strategy {
                prop::collection::vec(any::<GroupElement>(), num_elements)
                    .prop_map(ElementVector)
                    .boxed()
            }
        }

        impl Arbitrary for GroupPrgSeed {
            type Parameters = ();
            type Strategy = BoxedStrategy<Self>;

            fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
                (0..1000)
                    .prop_map(Integer::from)
                    .prop_map(GroupPrgSeed)
                    .boxed()
            }
        }

        pub fn seeds() -> impl Strategy<Value = Vec<GroupPrgSeed>> {
            prop::collection::vec(any::<GroupPrgSeed>(), 1..100)
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

        /// tests for seed-homomorphism with null seeds
        fn run_test_prg_eval_homomorphism_null_seed<P>(prg: P, seeds: Vec<P::Seed>)
        where
            P: SeedHomomorphicPRG,
            P::Seed: Eq + Clone + Debug + ops::Sub<Output = P::Seed>,
            P::Output: Eq + Clone + Debug + Hash,
        {
            let mut outputs: Vec<P::Output> = seeds.iter().map(|seed| prg.eval(seed)).collect();

            // null seed doesn't change the output
            let expected = prg.combine_outputs(outputs.clone());
            outputs.push(prg.eval(&prg.null_seed()));

            assert_eq!(prg.combine_outputs(outputs), expected);

            // null seed produces null output
            let seed = seeds[0].clone();
            let neg = prg.null_seed() - seeds[0].clone();
            assert_eq!(
                prg.combine_outputs(vec![prg.eval(&seed), prg.eval(&neg)]),
                prg.null_output()
            );
        }

        // group prg testing
        proptest! {
            #[test]
            fn test_group_prg_seed_random(prg: GroupPRG) {
                prg_tests::run_test_prg_seed_random(prg);
            }

            #[test]
            fn test_group_prg_eval_deterministic(prg: GroupPRG, seed: GroupPrgSeed)
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
            fn test_group_prg_eval_homomorphism_null(
                prg: GroupPRG, seeds in seeds(),
            ) {
                run_test_prg_eval_homomorphism_null_seed(prg, seeds);
            }

            // make sure that a null_seed doesn't change the output
            #[test]
            fn test_null_seed(
                prg: GroupPRG,
            ) {
                assert_eq!(prg.eval(&prg.null_seed()), prg.null_output());
            }

            #[test]
            fn test_group_prg_null_combine(prg: GroupPRG, seed: GroupPrgSeed) {
                prg_tests::run_test_prg_null_combine(prg, seed);
            }
        }

        proptest! {
            #[test]
            fn test_bytes_element_vec_roundtrip(data: Bytes) {
                let mut data:Vec<u8> = data.into();
                while data.len() % 31 != 0 {
                    data.push(0);
                }
                let data = Bytes::from(data);
                prop_assert_eq!(
                    data.clone(),
                    ElementVector::from(data).into()
                );
            }
        }
    }
}
