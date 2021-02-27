//! Spectrum implementation.
use crate::algebra::{Group, Monoid, SpecialExponentMonoid};
use crate::bytes::Bytes;
use crate::util::Sampleable;

use serde::{Deserialize, Serialize};

use std::cmp::max;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::hash::Hash;
use std::iter::repeat;

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;
#[cfg(any(test, feature = "testing"))]
use proptest_derive::Arbitrary;

use super::*;

use itertools::Itertools;

use std::ops::{BitXor, BitXorAssign};

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct ElementVector<G: Monoid>(pub Vec<G>);

impl<G: Group> ElementVector<G> {
    fn new(inner: Vec<G>) -> Self {
        ElementVector(inner)
    }
}

#[cfg(any(test, feature = "testing"))]
impl<G> Arbitrary for ElementVector<G>
where
    G: Debug + Arbitrary + Group + 'static,
{
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        prop::collection::vec(any::<G>(), 1..10)
            .prop_map(|v| {
                v.into_iter()
                    .filter(|g| g != &G::zero())
                    .collect::<Vec<_>>()
            })
            .prop_filter("element vector must be nonempty", |v| v.len() >= 1)
            .prop_map(ElementVector::new)
            .boxed()
    }
}

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
#[cfg_attr(any(test, feature = "testing"), derive(Arbitrary))]
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GroupPRG<G: Group + 'static> {
    generators: ElementVector<G>,
}

impl<G: Group> GroupPRG<G> {
    fn len(&self) -> usize {
        self.generators.0.len()
    }
}

impl<G> GroupPRG<G>
where
    G: Group + Sampleable,
{
    pub fn new(generators: ElementVector<G>) -> Self {
        GroupPRG { generators }
    }

    pub fn from_seed(num_elements: usize, seed: <G as Sampleable>::Seed) -> Self {
        let elements = G::sample_many_from_seed(&seed, num_elements);
        GroupPRG::new(ElementVector(elements))
    }
}

impl<G> PRG for GroupPRG<G>
where
    G: Group + SpecialExponentMonoid + Clone,
    G::Exponent: Sampleable + Clone,
{
    type Seed = G::Exponent;
    type Output = ElementVector<G>;

    /// generates a new (random) seed for the given PRG
    fn new_seed(&self) -> Self::Seed {
        Self::Seed::sample()
    }

    /// evaluates the PRG on the given seed
    fn eval(&self, seed: &Self::Seed) -> Self::Output {
        ElementVector(
            self.generators
                .0
                .iter()
                .cloned()
                .map(|g| g.pow(seed.clone()))
                .collect(),
        )
    }

    fn null_output(&self) -> Self::Output {
        ElementVector(repeat(G::zero()).take(self.len()).collect())
    }

    fn output_size(&self) -> usize {
        self.generators.0.len() * max(G::order_size_in_bytes() - 1, 1)
    }
}

impl<G> SeedHomomorphicPRG for GroupPRG<G>
where
    G: Group + SpecialExponentMonoid + Clone,
    G::Exponent: Sampleable + Monoid + Clone,
{
    fn null_seed(&self) -> Self::Seed {
        <G as SpecialExponentMonoid>::Exponent::zero()
    }

    fn combine_seeds(&self, seeds: Vec<Self::Seed>) -> Self::Seed {
        seeds.into_iter().fold(self.null_seed(), std::ops::Add::add)
    }

    fn combine_outputs(&self, outputs: &[&ElementVector<G>]) -> ElementVector<G> {
        let mut combined = self.null_output();
        for output in outputs {
            for (acc, val) in combined.0.iter_mut().zip(output.0.iter()) {
                *acc = acc.clone() + val.clone();
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
                .into_iter()
                .zip(rhs.0.into_iter())
                .map(|(element1, element2)| element1 + element2)
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
    G: Group + Clone,
{
    /// Apply the group operation on each component in the vector.
    // There's a mismatch between operations because we require that the PRG
    // output be XOR-able (and some properties on that).
    fn bitxor_assign(&mut self, rhs: ElementVector<G>) {
        self.0
            .iter_mut()
            .zip(rhs.0.into_iter())
            .for_each(|(element1, element2)| *element1 = element1.clone() + element2);
    }
}

/*
#[cfg(test)]
mod tests {
    extern crate rand;
    use super::super::tests as prg_tests;
    use super::*;
    use crate::constructions::IntModP;
    use std::collections::HashSet;
    use std::fmt::Debug;
    use std::ops;

    type Group = IntModP;

    pub fn seeds<G>() -> impl Strategy<Value = Vec<Group>>
    where
        G: Group + Arbitrary + 'static,
    {
        prop::collection::vec(any::<GroupPrgSeed<G>>(), 1..100)
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

*/
