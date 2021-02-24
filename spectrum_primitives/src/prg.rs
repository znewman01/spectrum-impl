//! Spectrum implementation.
use crate::bytes::Bytes;
use crate::field::{Field, FieldElement};
use crate::group::{Group, SampleableGroup};

use openssl::symm::{encrypt, Cipher};
use rand::prelude::*;
use rug::{integer::Order, Integer};
use serde::{Deserialize, Serialize};

use std::convert::TryFrom;
use std::fmt::{self, Debug};
use std::hash::Hash;
use std::iter::repeat;
use std::marker::PhantomData;

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

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
    fn combine_outputs(&self, outputs: &[&Self::Output]) -> Self::Output;
    fn null_seed(&self) -> Self::Seed;
}

#[cfg(test)]
mod tests {
    extern crate rand;
    use super::*;
    use proptest::prelude::TestCaseError;
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
            prg.combine_outputs(&[&prg.null_output(), &prg.null_output()])
        );

        // ensure combine(null, eval) = eval
        assert_eq!(
            prg.eval(&seed),
            prg.combine_outputs(&[&prg.eval(&seed), &prg.null_output()])
        );

        // ensure combine(eval, null) = eval
        assert_eq!(
            prg.eval(&seed),
            prg.combine_outputs(&[&prg.null_output(), &prg.eval(&seed)])
        );
    }

    pub fn run_test_prg_seed_random<P>(prg: P) -> Result<(), TestCaseError>
    where
        P: PRG,
        P::Seed: Eq + Debug,
        P::Output: Eq + Debug,
    {
        prop_assert_ne!(prg.new_seed(), prg.new_seed());
        Ok(())
    }

    pub fn run_test_prg_eval_deterministic<P>(prg: P, seed: P::Seed)
    where
        P: PRG,
        P::Seed: Eq + Debug,
        P::Output: Eq + Debug,
    {
        assert_eq!(prg.eval(&seed), prg.eval(&seed));
    }

    pub fn run_test_prg_eval_random<P>(prg: P, seeds: &[P::Seed]) -> Result<(), TestCaseError>
    where
        P: PRG,
        P::Seed: Eq + Debug,
        P::Output: Eq + Debug + Hash,
    {
        let results: HashSet<_> = seeds.iter().map(|s| prg.eval(s)).collect();
        prop_assert_eq!(results.len(), seeds.len());
        Ok(())
    }
}

pub mod aes {
    use super::*;

    /// PRG uses AES to expand a seed to desired length
    #[derive(Clone, PartialEq, Copy, Serialize, Deserialize)]
    pub struct AESPRG {
        seed_size: usize,
        eval_size: usize,
        #[serde(skip, default = "Cipher::aes_128_ctr")]
        cipher: Cipher,
    }

    impl fmt::Debug for AESPRG {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("AESPRG")
                .field("seed_size", &self.seed_size)
                .field("eval_size", &self.eval_size)
                .finish()
        }
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
                cipher: Cipher::aes_128_ctr(),
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
            let mut ciphertext = encrypt(
                self.cipher,
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

    /// Test helpers
    #[cfg(any(test, feature = "testing"))]
    mod testing {
        use std::ops::Range;

        pub const SIZES: Range<usize> = 16..1000; // in bytes
        pub const SEED_SIZE: usize = 16; // in bytes
    }

    #[cfg(any(test, feature = "testing"))]
    impl Arbitrary for AESPRG {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            use testing::*;
            SIZES
                .prop_map(|output_size| AESPRG::new(SEED_SIZE, output_size))
                .boxed()
        }
    }

    #[cfg(any(test, feature = "testing"))]
    impl Arbitrary for AESSeed {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            use testing::*;
            prop::collection::vec(any::<u8>(), SEED_SIZE)
                .prop_map(|data| AESSeed { bytes: data.into() })
                .boxed()
        }
    }

    #[cfg(test)]
    mod tests {
        extern crate rand;
        use super::super::tests as prg_tests;
        use super::*;

        proptest! {
            #[test]
            fn test_aes_prg_seed_random(prg in any::<AESPRG>()) {
               prg_tests::run_test_prg_seed_random(prg)?;
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
                prg_tests::run_test_prg_eval_random(prg, &seeds)?;
            }
        }
    }
}

pub mod group {
    use super::aes::AESSeed;
    use super::*;

    use itertools::Itertools;

    use std::ops::{self, BitXor, BitXorAssign};

    #[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
    pub struct ElementVector<G>(pub Vec<G>);

    impl<G> ElementVector<G>
    where
        G: Group + Into<Bytes>,
    {
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

    impl<G> From<Bytes> for ElementVector<G>
    where
        G: Group + TryFrom<Bytes>,
        G::Error: Debug,
    {
        fn from(bytes: Bytes) -> Self {
            // Turns out the group can't represent a lot of 32-byte values
            // because its modulus is < 2^32.
            // We use (very unnatural) 31-byte chunks so that
            // element_from_bytes() succeeds.
            let chunk_size = G::order_size_in_bytes() - 1;
            ElementVector(
                bytes
                    .into_iter()
                    .chunks(chunk_size)
                    .into_iter()
                    .map(|data| {
                        let mut data: Vec<u8> = data.collect();
                        while data.len() < G::order_size_in_bytes() {
                            data.push(0);
                        }
                        let data = Bytes::from(data);
                        G::try_from(data).expect("chunk size chosen s.t. this never fails")
                    })
                    .collect(),
            )
        }
    }

    // Implementation of a group-based PRG
    #[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
    pub struct GroupPRG<G> {
        generators: ElementVector<G>,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct GroupPrgSeed<G> {
        value: Integer,
        phantom: PhantomData<G>,
    }

    impl<G> From<Integer> for GroupPrgSeed<G>
    where
        G: Group,
    {
        fn from(value: Integer) -> Self {
            let mut value: Integer = value;
            while value < 0 {
                value += G::order();
            }
            if value >= G::order() {
                value %= G::order();
            }
            Self {
                value,
                phantom: PhantomData,
            }
        }
    }

    impl<G> GroupPrgSeed<G> {
        pub fn value(self) -> Integer {
            self.value
        }
    }

    impl<G> ops::Sub for GroupPrgSeed<G>
    where
        G: Group,
    {
        type Output = Self;

        fn sub(self, other: Self) -> Self {
            let mut value = self.value - other.value;
            if value < 0 {
                value += G::order();
            }
            GroupPrgSeed::from(value)
        }
    }

    impl<G> ops::SubAssign for GroupPrgSeed<G>
    where
        G: Group,
    {
        #[allow(clippy::suspicious_op_assign_impl)]
        fn sub_assign(&mut self, other: Self) {
            self.value -= other.value;
            if self.value < 0 {
                self.value += G::order();
            }
        }
    }

    impl<G> ops::Add for GroupPrgSeed<G>
    where
        G: Group,
    {
        type Output = Self;

        fn add(self, other: Self) -> Self {
            let mut value = self.value + other.value;
            if value >= G::order() {
                value -= G::order();
            }
            GroupPrgSeed::from(value)
        }
    }

    impl<G> ops::AddAssign for GroupPrgSeed<G>
    where
        G: Group,
    {
        #[allow(clippy::suspicious_op_assign_impl)]
        fn add_assign(&mut self, other: Self) {
            self.value += other.value;
            if self.value >= G::order() {
                self.value -= G::order();
            }
        }
    }

    impl<G> Into<Vec<u8>> for GroupPrgSeed<G> {
        fn into(self) -> Vec<u8> {
            self.value.to_string_radix(10).into_bytes()
        }
    }

    impl<G> From<Vec<u8>> for GroupPrgSeed<G>
    where
        G: Group,
    {
        fn from(data: Vec<u8>) -> Self {
            let data = String::from_utf8(data).unwrap();
            let value: Integer = Integer::parse_radix(&data, 10).unwrap().into();
            GroupPrgSeed::from(value)
        }
    }

    impl<G> GroupPRG<G>
    where
        G: Group + SampleableGroup,
    {
        pub fn new(generators: ElementVector<G>) -> Self {
            GroupPRG { generators }
        }

        pub fn from_aes_seed(num_elements: usize, seed: AESSeed) -> Self {
            GroupPRG::new(ElementVector(G::generators(num_elements, &seed)))
        }
    }

    impl<G> PRG for GroupPRG<G>
    where
        G: Group + Clone,
    {
        type Seed = GroupPrgSeed<G>;
        type Output = ElementVector<G>;

        /// generates a new (random) seed for the given PRG
        fn new_seed(&self) -> Self::Seed {
            let mut rand_bytes = vec![0; G::order_size_in_bytes()];
            thread_rng().fill_bytes(&mut rand_bytes);
            GroupPrgSeed::from(Integer::from_digits(&rand_bytes.as_ref(), Order::LsfLe))
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
                repeat(G::identity())
                    .take(self.generators.0.len())
                    .collect(),
            )
        }

        fn output_size(&self) -> usize {
            self.generators.0.len() * (G::order_size_in_bytes() - 1)
        }
    }

    impl<G> SeedHomomorphicPRG for GroupPRG<G>
    where
        G: Group + Clone,
    {
        fn null_seed(&self) -> Self::Seed {
            GroupPrgSeed::from(Integer::from(0))
        }

        fn combine_seeds(&self, seeds: Vec<GroupPrgSeed<G>>) -> GroupPrgSeed<G> {
            let seeds: Vec<Integer> = seeds.into_iter().map(|s| s.value()).collect();
            GroupPrgSeed::from(Integer::from(Integer::sum(seeds.iter())) % G::order())
        }

        fn combine_outputs(&self, outputs: &[&ElementVector<G>]) -> ElementVector<G> {
            let mut combined = self.null_output();
            for output in outputs {
                for (acc, val) in combined.0.iter_mut().zip(output.0.iter()) {
                    *acc = acc.op(val);
                }
            }
            combined
        }
    }

    // TODO: should be try_into()
    impl<G> Into<Bytes> for ElementVector<G>
    where
        G: Group + Into<Bytes>,
    {
        fn into(self) -> Bytes {
            let chunk_size = G::order_size_in_bytes() - 1;
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

    impl<G> BitXor<ElementVector<G>> for ElementVector<G>
    where
        G: Group,
    {
        type Output = ElementVector<G>;

        // apply the group operation on each component in the vector
        fn bitxor(self, rhs: ElementVector<G>) -> ElementVector<G> {
            ElementVector(
                self.0
                    .iter()
                    .zip(rhs.0.iter())
                    .map(|(element1, element2)| element1.op(element2))
                    .collect(),
            )
        }
    }

    impl<G> Into<Vec<u8>> for ElementVector<G>
    where
        G: Group + Into<Bytes>,
    {
        fn into(self) -> Vec<u8> {
            let chunk_size = G::order_size_in_bytes();
            // outputs all the elements in the vector concatenated as a sequence of bytes
            // assumes that every element is < 2^(8*31)
            let mut all_bytes = Vec::with_capacity(chunk_size * self.0.len());
            for element in self.0.into_iter() {
                let bytes: Bytes = element.into();
                let mut bytes: Vec<u8> = bytes.into();
                all_bytes.append(&mut bytes);
            }
            all_bytes
        }
    }

    impl<G> From<Vec<u8>> for ElementVector<G>
    where
        G: Group + TryFrom<Bytes>,
        G::Error: Debug,
    {
        fn from(bytes: Vec<u8>) -> Self {
            let chunk_size = G::order_size_in_bytes();
            // outputs all the elements in the vector concatenated as a sequence of bytes
            let mut elements = vec![];
            for chunk in bytes.into_iter().chunks(chunk_size).into_iter() {
                elements.push(G::try_from(Bytes::from(chunk.collect::<Vec<u8>>())).unwrap());
            }
            ElementVector(elements)
        }
    }

    impl<G> BitXorAssign<ElementVector<G>> for ElementVector<G>
    where
        G: Group,
    {
        /// Apply the group operation on each component in the vector.
        // There's a mismatch between operations because we require that the PRG
        // output be XOR-able (and some properties on that).
        fn bitxor_assign(&mut self, rhs: ElementVector<G>) {
            self.0
                .iter_mut()
                .zip(rhs.0.iter())
                .for_each(|(element1, element2)| *element1 = element1.op(element2));
        }
    }

    /// Test helpers
    #[cfg(any(test, feature = "testing"))]
    mod testing {
        use std::ops::Range;

        pub const GROUP_PRG_NUM_SEEDS: Range<usize> = 1..10; // # group elements
    }

    #[cfg(any(test, feature = "testing"))]
    impl<G> Arbitrary for GroupPRG<G>
    where
        G: Group + Debug + Clone + SampleableGroup + 'static,
    {
        type Parameters = Option<(usize, aes::AESSeed)>;
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(params: Self::Parameters) -> Self::Strategy {
            use testing::*;
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

    #[cfg(any(test, feature = "testing"))]
    impl<G> Arbitrary for ElementVector<G>
    where
        G: Arbitrary + 'static,
    {
        type Parameters = prop::collection::SizeRange;
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(num_elements: Self::Parameters) -> Self::Strategy {
            prop::collection::vec(any::<G>(), num_elements)
                .prop_map(ElementVector)
                .boxed()
        }
    }

    #[cfg(any(test, feature = "testing"))]
    impl<G> Arbitrary for GroupPrgSeed<G>
    where
        G: Group + Debug + 'static,
    {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            (0..1000)
                .prop_map(Integer::from)
                .prop_map(GroupPrgSeed::from)
                .boxed()
        }
    }

    #[cfg(test)]
    mod tests {
        extern crate rand;
        use super::super::tests as prg_tests;
        use super::*;
        use crate::group::GroupElement;
        use std::collections::HashSet;
        use std::fmt::Debug;
        use std::ops;

        pub fn seeds<G>() -> impl Strategy<Value = Vec<GroupPrgSeed<G>>>
        where
            G: Group + Arbitrary + 'static,
        {
            prop::collection::vec(any::<GroupPrgSeed<G>>(), 1..100)
        }

        /// tests for seed-homomorphism: G(s1) ^ G(s2) = G(s1 * s2)
        fn run_test_prg_eval_homomorphism<P>(prg: P, seeds: Vec<P::Seed>)
        where
            P: SeedHomomorphicPRG,
            P::Seed: Eq + Clone + Debug,
            P::Output: Eq + Clone + Debug + Hash,
        {
            let output_src: Vec<_> = seeds.iter().map(|seed| prg.eval(seed)).collect();
            let outputs: Vec<_> = output_src.iter().collect();

            assert_eq!(
                prg.combine_outputs(&outputs),
                prg.eval(&prg.combine_seeds(seeds))
            );
        }

        /// tests for seed-homomorphism with null seeds
        fn run_test_prg_eval_homomorphism_null_seed<P>(
            prg: P,
            seeds: Vec<P::Seed>,
        ) -> Result<(), TestCaseError>
        where
            P: SeedHomomorphicPRG,
            P::Seed: Eq + Clone + Debug + ops::Sub<Output = P::Seed>,
            P::Output: Eq + Clone + Debug + Hash,
        {
            let output_src: Vec<_> = seeds.iter().map(|seed| prg.eval(seed)).collect();
            let mut outputs: Vec<_> = output_src.iter().collect();

            // null seed doesn't change the output
            let expected = prg.combine_outputs(&outputs);
            let output = prg.eval(&prg.null_seed());
            outputs.push(&output);

            prop_assert_eq!(prg.combine_outputs(&outputs), expected);

            // null seed produces null output
            let seed = seeds[0].clone();
            let neg = prg.null_seed() - seeds[0].clone();
            prop_assert_eq!(
                prg.combine_outputs(&[&prg.eval(&seed), &prg.eval(&neg)]),
                prg.null_output()
            );

            Ok(())
        }

        // group prg testing
        proptest! {
            #[test]
            fn test_group_prg_seed_random(prg: GroupPRG<GroupElement>) {
                prg_tests::run_test_prg_seed_random(prg)?;
            }

            #[test]
            fn test_group_prg_eval_deterministic(prg: GroupPRG<GroupElement>, seed: GroupPrgSeed<GroupElement>)
            {
                prg_tests::run_test_prg_eval_deterministic(prg, seed);
            }

            #[test]
            fn test_group_prg_eval_random(prg: GroupPRG<GroupElement>, seeds in seeds())
             {
                let unique: HashSet<_> = seeds.iter().cloned().collect();
                prop_assume!(unique.len() == seeds.len(), "eval must be different for different seeds");
                prg_tests::run_test_prg_eval_random(prg, &seeds)?;
            }
        }

        // seed homomorphic prg tests
        proptest! {
            #[test]
            fn test_group_prg_eval_homomorphism(
                prg: GroupPRG<GroupElement>, seeds in seeds(),
            ) {
                run_test_prg_eval_homomorphism(prg, seeds);
            }

            #[test]
            fn test_group_prg_eval_homomorphism_null(
                prg: GroupPRG<GroupElement>, seeds in seeds(),
            ) {
                run_test_prg_eval_homomorphism_null_seed(prg, seeds)?;
            }

            // make sure that a null_seed doesn't change the output
            #[test]
            fn test_null_seed(
                prg: GroupPRG<GroupElement>,
            ) {
                prop_assert_eq!(prg.eval(&prg.null_seed()), prg.null_output());
            }

            #[test]
            fn test_group_prg_null_combine(prg: GroupPRG<GroupElement>, seed: GroupPrgSeed<GroupElement>) {
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
                    ElementVector::<GroupElement>::from(data).into()
                );
            }
        }
    }
}
