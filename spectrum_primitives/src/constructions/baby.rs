//! Simple example field, useful for testing/debugging.
use std::convert::{TryFrom, TryInto};
use std::iter::{repeat_with, Sum};
use std::ops;

use rug::Integer;

use crate::algebra::{Field, Group, Monoid, SpecialExponentMonoid};
use crate::util::Sampleable;

/// A `u8` wrapper that implements a Group (and maybe field).
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct IntMod<const N: u8> {
    inner: u8,
}

// If we wanted to really go for it, we'd verify that N was prime. I think
// Rust's type system can do it but I'm not quite that masochistic...
impl<const N: u8> Field for IntMod<N> {
    fn one() -> Self {
        return Self { inner: 1 };
    }

    fn mul_invert(&self) -> Self {
        if self.inner == 0 {
            panic!("Zero has no multiplicative inverse");
        }
        // Could implement extended Euclidean algorithm, or...
        for rhs in 0..N {
            if ((self.inner as u16) * (rhs as u16)) % (N as u16) == 1 {
                return Self { inner: rhs };
            }
        }
        panic!("No inverse found!");
    }
}

impl<const N: u8> Monoid for IntMod<N> {
    fn zero() -> Self {
        Self { inner: 0 }
    }
}

impl<const N: u8> Group for IntMod<N> {
    fn order() -> rug::Integer {
        Integer::from(N)
    }
}

impl<const N: u8> SpecialExponentMonoid for IntMod<N> {
    type Exponent = IntMod<N>;

    fn pow(&self, rhs: IntMod<N>) -> Self {
        let inner = (Integer::from(self.inner) * Integer::from(rhs.inner)) % Self::order();
        Self {
            inner: inner.try_into().unwrap(),
        }
    }
}

impl<const N: u8> ops::Add for IntMod<N> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        // Work in u16s so we don't have to worry about overflow
        let inner = ((self.inner as u16) + (rhs.inner as u16)) % (N as u16);
        Self {
            inner: inner.try_into().unwrap(),
        }
    }
}

impl<const N: u8> ops::Sub for IntMod<N> {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        self + (-rhs)
    }
}

impl<const N: u8> Sum for IntMod<N> {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        let mut total = <Self as Monoid>::zero();
        iter.for_each(|value| total += value);
        total
    }
}

impl<const N: u8> ops::AddAssign for IntMod<N> {
    fn add_assign(&mut self, rhs: Self) {
        self.inner = (self.clone() + rhs).inner;
    }
}

impl<const N: u8> ops::Neg for IntMod<N> {
    type Output = Self;

    fn neg(self) -> Self {
        let inner = N - self.inner;
        Self { inner }
    }
}

impl<const N: u8> ops::Mul for IntMod<N> {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        // Work in u16s so we don't have to worry about overflow
        let inner = ((self.inner as u16) * (rhs.inner as u16)) % (N as u16);
        Self {
            inner: inner.try_into().unwrap(),
        }
    }
}

impl<const N: u8> TryFrom<u8> for IntMod<N> {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, ()> {
        if value >= Self::order() {
            return Err(());
        }
        Ok(Self { inner: value })
    }
}

// impl<const N: u8> Into<Integer> for IntMod<N> {
//     fn into(self) -> Integer {
//         self.inner.into()
//     }
// }

use rand::prelude::*;

impl<const N: u8> Sampleable for IntMod<N> {
    type Seed = <StdRng as SeedableRng>::Seed;

    fn sample() -> Self {
        thread_rng().gen_range(0..N).try_into().unwrap()
    }

    fn sample_many_from_seed(seed: &Self::Seed, n: usize) -> Vec<Self> {
        let mut rng = <StdRng as SeedableRng>::from_seed(seed.clone());
        repeat_with(|| rng.gen_range(0..N))
            .take(n)
            .map(Self::try_from)
            .map(Result::unwrap)
            .collect()
    }
}

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;
#[cfg(any(test, feature = "testing"))]
impl<const N: u8> Arbitrary for IntMod<N> {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use std::ops::Range;
        let range: Range<u8> = 0..N;
        range
            .prop_map(Self::try_from)
            .prop_map(Result::unwrap)
            .boxed()
    }
}

#[cfg(test)]
mod test_int_mod {
    use super::*;
    use crate::dpf::MultiKeyDpf;
    use crate::prg::{GroupPRG, SeedHomomorphicPRG, PRG};

    type IntModP = IntMod<11>;
    check_field_laws!(IntModP);
    check_monoid_custom_exponent!(IntModP);
    check_sampleable!(IntModP);
    check_shareable!(IntModP);

    check_linearly_shareable!(IntModP);
    check_prg!(GroupPRG<IntModP>);
    check_seed_homomorphic_prg!(GroupPRG<IntModP>);
    check_dpf!(MultiKeyDpf<GroupPRG<IntModP>>);
}
