//! Spectrum implementation.
use super::*;
use crate::algebra::{Group, Monoid, SpecialExponentMonoid};
use crate::util::Sampleable;

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use std::convert::TryFrom;
use std::fmt::Debug;
use std::hash::Hash;
use std::iter::repeat;
use std::ops::{Add, BitXor, BitXorAssign};

#[cfg(any(test, feature = "testing"))]
use proptest::{collection::SizeRange, prelude::*};
#[cfg(any(test, feature = "testing"))]
use proptest_derive::Arbitrary;

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct ElementVector<G>(pub Vec<G>);

impl<G: Group> ElementVector<G> {
    pub fn new(inner: Vec<G>) -> Self {
        ElementVector(inner)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<G> Add for ElementVector<G>
where
    G: Add<Output = G>,
{
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let inner = Iterator::zip(self.0.into_iter(), rhs.0.into_iter())
            .map(|(x, y)| x + y)
            .collect();
        Self(inner)
    }
}

impl<G: Monoid> Monoid for ElementVector<G> {
    fn zero() -> Self {
        panic!("not enough information (don't know the right length)");
    }
}

impl<G> SpecialExponentMonoid for ElementVector<G>
where
    G: SpecialExponentMonoid,
    G::Exponent: Clone,
{
    type Exponent = G::Exponent;

    fn pow(&self, exp: Self::Exponent) -> Self {
        Self(self.0.iter().map(|x| x.pow(exp.clone())).collect())
    }
}

#[cfg(any(test, feature = "testing"))]
impl<G> Arbitrary for ElementVector<G>
where
    G: Debug + Arbitrary + Group + 'static,
{
    type Parameters = Option<usize>;
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(size: Self::Parameters) -> Self::Strategy {
        let range = size
            .map(SizeRange::from)
            .unwrap_or_else(|| SizeRange::from(1..5));
        prop::collection::vec(
            any::<G>().prop_filter("nonzero", |g| g != &G::zero()),
            range,
        )
        .prop_map(ElementVector::new)
        .boxed()
    }
}

impl<G> ElementVector<G>
where
    G: Group + Into<Vec<u8>>,
{
    pub fn hash_all(self) -> Vec<u8> {
        let mut hasher = blake3::Hasher::new();
        for element in self.0 {
            let chunk: Vec<u8> = element.into();
            hasher.update(&chunk);
        }
        let data: [u8; 32] = hasher.finalize().into();
        data.to_vec()
    }
}

// Implementation of a group-based PRG
#[cfg_attr(any(test, feature = "testing"), derive(Arbitrary))]
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GroupPrg<G: Group + 'static> {
    generators: ElementVector<G>,
}

impl<G: Group> GroupPrg<G> {
    fn len(&self) -> usize {
        self.generators.0.len()
    }
}

impl<G> GroupPrg<G>
where
    G: Group + Sampleable,
{
    pub fn new(generators: ElementVector<G>) -> Self {
        GroupPrg { generators }
    }

    pub fn from_seed(num_elements: usize, seed: <G as Sampleable>::Seed) -> Self {
        let elements = G::sample_many_from_seed(&seed, num_elements);
        GroupPrg::new(ElementVector(elements))
    }
}

impl<G> Prg for GroupPrg<G>
where
    G: Group + SpecialExponentMonoid + Clone,
    G::Exponent: Sampleable + Clone,
{
    type Seed = G::Exponent;
    type Output = ElementVector<G>;

    /// generates a new (random) seed for the given PRG
    fn new_seed() -> Self::Seed {
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
        self.generators.len()
    }
}

impl<G> SeedHomomorphicPrg for GroupPrg<G>
where
    G: Group + SpecialExponentMonoid + Clone,
    G::Exponent: Sampleable + Monoid + Clone,
{
    fn null_seed() -> Self::Seed {
        <G as SpecialExponentMonoid>::Exponent::zero()
    }

    fn combine_seeds(&self, seeds: Vec<Self::Seed>) -> Self::Seed {
        seeds
            .into_iter()
            .fold(Self::null_seed(), std::ops::Add::add)
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

// // TODO: should be try_from()
// impl<G> From<ElementVector<G>> for Bytes
// where
//     G: Group + Into<Bytes>,
// {
//     fn from(value: ElementVector<G>) -> Bytes {
//         let chunk_size = G::order_size_in_bytes() - 1;
//         // outputs all the elements in the vector concatenated as a sequence of bytes
//         // assumes that every element is < 2^(8*31)
//         let mut all_bytes = Vec::with_capacity(chunk_size * value.0.len());
//         for element in value.0.into_iter() {
//             let bytes: Bytes = element.into();
//             let bytes: Vec<u8> = bytes.into();
//             assert_eq!(bytes.clone()[31], 0);
//             let bytes = Bytes::from(bytes[0..31].to_vec());
//             all_bytes.append(&mut bytes.into());
//         }
//         Bytes::from(all_bytes)
//     }
// }

impl<G> BitXor<ElementVector<G>> for ElementVector<G>
where
    G: Group,
{
    type Output = ElementVector<G>;

    // apply the group operation on each component in the vector
    #[allow(clippy::suspicious_arithmetic_impl)]
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

impl<G> From<ElementVector<G>> for Vec<u8>
where
    G: Group + Into<Vec<u8>>,
{
    fn from(value: ElementVector<G>) -> Vec<u8> {
        let chunk_size = G::order_size_in_bytes();
        // outputs all the elements in the vector concatenated as a sequence of bytes
        // assumes that every element is < 2^(8*31)
        let mut all_bytes = Vec::with_capacity(chunk_size * value.0.len());
        for element in value.0.into_iter() {
            let mut bytes: Vec<u8> = element.into();
            all_bytes.append(&mut bytes);
        }
        all_bytes
    }
}

impl<G> TryFrom<Vec<u8>> for ElementVector<G>
where
    G: Group + TryFrom<Vec<u8>>,
    G::Error: Debug,
    (): From<G::Error>,
{
    type Error = ();

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        let chunk_size = G::order_size_in_bytes();
        // outputs all the elements in the vector concatenated as a sequence of bytes
        let elements = bytes
            .into_iter()
            .chunks(chunk_size)
            .into_iter()
            .map(|chunk| G::try_from(chunk.collect::<Vec<u8>>()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ElementVector(elements))
    }
}

impl<G> BitXorAssign<ElementVector<G>> for ElementVector<G>
where
    G: Group + Clone,
{
    /// Apply the group operation on each component in the vector.
    // There's a mismatch between operations because we require that the PRG
    // output be XOR-able (and some properties on that).
    #[allow(clippy::suspicious_op_assign_impl)]
    fn bitxor_assign(&mut self, rhs: ElementVector<G>) {
        self.0
            .iter_mut()
            .zip(rhs.0.into_iter())
            .for_each(|(element1, element2)| *element1 = element1.clone() + element2);
    }
}
