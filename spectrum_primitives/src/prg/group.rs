//! Spectrum implementation.
use crate::algebra::Group;
use crate::bytes::Bytes;
use crate::util::Sampleable;

use rand::prelude::*;
use rug::{integer::Order, Integer};
use serde::{Deserialize, Serialize};

use std::cmp::max;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::hash::Hash;
use std::iter::repeat;
use std::marker::PhantomData;

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;
#[cfg(any(test, feature = "testing"))]
use proptest_derive::Arbitrary;

use super::*;

use itertools::Itertools;

use std::ops::{self, BitXor, BitXorAssign};

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct ElementVector<G: Group>(pub Vec<G>);

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
        prop::collection::vec(
            any::<G>().prop_filter("nonzero", |g| g != &G::zero()),
            1..1000,
        )
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

impl<G: Group> ops::Neg for GroupPrgSeed<G> {
    type Output = Self;

    fn neg(self) -> Self {
        let value = G::order() - self.value;
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

impl<G> ops::Mul<Integer> for GroupPrgSeed<G>
where
    G: Group,
{
    type Output = Self;

    fn mul(self, other: Integer) -> Self {
        Self::from((self.value * other) % &G::order())
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

impl<G> Group for GroupPrgSeed<G>
where
    G: Group,
{
    fn order() -> Integer {
        G::order()
    }

    fn zero() -> Self {
        Self {
            value: Integer::from(0),
            phantom: PhantomData,
        }
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
                .cloned()
                .map(|g| g * seed.clone().value())
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

#[cfg(any(test, feature = "testing"))]
impl<G> Arbitrary for GroupPrgSeed<G>
where
    G: Group + Debug + 'static,
{
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        // TODO: use the full range 2..order
        let order: u128 = G::order().to_u128().unwrap_or(u128::MAX);
        // Can't do 0:  wk
        (2..order)
            .prop_map(Integer::from)
            .prop_map(GroupPrgSeed::from)
            .boxed()
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
