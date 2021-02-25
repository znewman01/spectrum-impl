//! Spectrum implementation.
use crate::group::{GroupElement, Sampleable};

use rand::{thread_rng, Rng};
use rug::Integer;
// use rug::{integer::Order, Integer};

use std::convert::{TryFrom, TryInto};

#[cfg(feature = "proto")]
use crate::proto;

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

// const BYTE_ORDER: Order = Order::LsfLe;

pub trait FieldTrait: Eq + PartialEq {
    /// Add in the field>
    fn add(&self, rhs: &Self) -> Self;
    /// Take the additive inverse.
    fn neg(&self) -> Self;
    /// The additive identity.
    fn zero() -> Self;
    /// Multiply in the field.
    fn mul(&self, rhs: &Self) -> Self;
    /// Take the multiplicative inverse.
    ///
    /// Panics if you pass zero().
    fn mul_invert(&self) -> Self;
    /// The multiplicative identity.
    fn one() -> Self;
}

// If I were really clever, I'd give a general implementation with two
// commutative groups, over Self and NonZero<Self> (plus distributivity).

// TODO: When const generics (RFC2000) stabilize in Rust, parameterize over
// modulus
/// Simple example field, useful for testing/debugging.
#[derive(Debug, Clone, Eq, PartialEq)]
struct IntMod5 {
    inner: u8,
}

impl IntMod5 {
    const fn order() -> u8 {
        5
    }
}

impl FieldTrait for IntMod5 {
    fn add(&self, rhs: &IntMod5) -> IntMod5 {
        IntMod5 {
            inner: (self.inner + rhs.inner) % 5,
        }
    }
    fn neg(&self) -> IntMod5 {
        IntMod5 {
            inner: (5 - self.inner) % 5,
        }
    }
    fn zero() -> Self {
        return IntMod5 { inner: 0 };
    }
    fn mul(&self, rhs: &IntMod5) -> IntMod5 {
        IntMod5 {
            inner: (self.inner * rhs.inner) % 5,
        }
    }
    fn one() -> Self {
        return IntMod5 { inner: 1 };
    }

    fn mul_invert(&self) -> Self {
        // Could implement extended Euclidean algorithm, or...
        let value = match self.inner {
            0 => {
                panic!("Zero has no multiplicative inverse");
            }
            1 => 1,
            2 => 3,
            3 => 2,
            4 => 4,
            _ => {
                panic!("Invalid IntMod5");
            }
        };
        IntMod5 { inner: value }
    }
}

impl Sampleable for IntMod5 {
    fn rand_element() -> Self {
        thread_rng().gen_range(0, Self::order()).try_into().unwrap()
    }

    fn generators(_num: usize, _seed: &crate::prg::aes::AESSeed) -> Vec<Self>
    where
        Self: Sized,
    {
        todo!()
    }
}

impl TryFrom<u8> for IntMod5 {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, ()> {
        if value >= Self::order() {
            return Err(());
        }
        Ok(Self { inner: value })
    }
}

#[cfg(test)]
mod test_helpers {
    use super::*;
    use std::fmt::Debug;
    use std::iter::repeat_with;
    use std::{collections::HashSet, hash::Hash};

    pub(super) fn run_test_add_associative<F>(a: F, b: F, c: F) -> Result<(), TestCaseError>
    where
        F: FieldTrait + Debug,
    {
        prop_assert_eq!(a.add(&b).add(&c), a.add(&b.add(&c)));
        Ok(())
    }

    pub(super) fn run_test_add_commutative<F>(a: F, b: F) -> Result<(), TestCaseError>
    where
        F: FieldTrait + Debug,
    {
        prop_assert_eq!(a.add(&b), b.add(&a));
        Ok(())
    }

    pub(super) fn run_test_add_identity<F>(a: F) -> Result<(), TestCaseError>
    where
        F: FieldTrait + Debug,
    {
        prop_assert_eq!(a.add(&F::zero()), a);
        Ok(())
    }

    pub(super) fn run_test_add_inverse<F>(a: F) -> Result<(), TestCaseError>
    where
        F: FieldTrait + Debug,
    {
        prop_assert_eq!(a.add(&a.neg()), F::zero());
        Ok(())
    }

    pub(super) fn run_test_mul_associative<F>(a: F, b: F, c: F) -> Result<(), TestCaseError>
    where
        F: FieldTrait + Debug,
    {
        prop_assert_eq!(a.mul(&b).mul(&c), a.mul(&b.mul(&c)));
        Ok(())
    }

    pub(super) fn run_test_mul_commutative<F>(a: F, b: F) -> Result<(), TestCaseError>
    where
        F: FieldTrait + Debug,
    {
        prop_assert_eq!(a.mul(&b), b.mul(&a));
        Ok(())
    }

    pub(super) fn run_test_mul_identity<F>(a: F) -> Result<(), TestCaseError>
    where
        F: FieldTrait + Debug,
    {
        prop_assert_eq!(a.mul(&F::one()), a);
        Ok(())
    }

    pub(super) fn run_test_mul_inverse<F>(a: F) -> Result<(), TestCaseError>
    where
        F: FieldTrait + Debug,
    {
        prop_assume!(a != F::zero());
        prop_assert_eq!(a.mul(&a.mul_invert()), F::one());
        Ok(())
    }

    pub(super) fn run_test_distributive<F>(a: F, b: F, c: F) -> Result<(), TestCaseError>
    where
        F: FieldTrait + Debug,
    {
        prop_assert_eq!(a.mul(&b.add(&c)), a.mul(&b).add(&a.mul(&c)));
        Ok(())
    }

    pub(super) fn run_test_rand_not_deterministic<F>() -> Result<(), TestCaseError>
    where
        F: Sampleable + Hash + Eq,
    {
        let elements: HashSet<_> = repeat_with(|| F::rand_element()).take(10).collect();
        prop_assert!(
            elements.len() > 1,
            "Many random elements should not all be the same."
        );
        Ok(())
    }
}

#[cfg(test)]
impl Arbitrary for IntMod5 {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use std::ops::Range;
        let range: Range<u8> = 0..Self::order();
        range
            .prop_map(Self::try_from)
            .prop_map(Result::unwrap)
            .boxed()
    }
}

#[cfg(test)]
mod test_mod5 {
    // use super::test_helpers::*;
    // use super::*;

    /*
    proptest! {
        #[test]
        fn test_add_associative(a: IntMod5, b: IntMod5, c: IntMod5) {
            run_test_add_associative(a, b, c)?;
        }

        #[test]
        fn test_add_commutative(a: IntMod5, b: IntMod5) {
            run_test_add_commutative(a, b)?;
        }

        #[test]
        fn test_add_identity(a: IntMod5) {
            run_test_add_identity(a)?;
        }

        #[test]
        fn test_add_inverse(a: IntMod5) {
            run_test_add_inverse(a)?;
        }

        #[test]
        fn test_mul_associative(a: IntMod5, b: IntMod5, c: IntMod5) {
            run_test_mul_associative(a, b, c)?;
        }

        #[test]
        fn test_mul_commutative(a: IntMod5, b: IntMod5) {
            run_test_mul_commutative(a, b)?;
        }

        #[test]
        fn test_mul_identity(a: IntMod5) {
            run_test_mul_identity(a)?;
        }

        #[test]
        fn test_mul_inverse(a: IntMod5) {
            run_test_mul_inverse(a)?;
        }

        #[test]
        fn test_distributive(a: IntMod5, b: IntMod5, c: IntMod5) {
            run_test_distributive(a, b, c)?;
        }

       #[test]
       fn run_test_rand_not_deterministic() {
           run_test_rand_not_deterministic::<IntMod5>()?;
       }
    }
    */
}

/// Ints mod 2^128 - 159
#[derive(Debug, Clone, Eq, PartialEq)]
struct IntMod128BitPrime {
    inner: u128,
}

impl IntMod128BitPrime {
    const fn order() -> u128 {
        u128::MAX - 158
    }
}

impl TryFrom<u128> for IntMod128BitPrime {
    type Error = ();

    fn try_from(value: u128) -> Result<Self, ()> {
        if value >= Self::order() {
            return Err(());
        }
        Ok(Self { inner: value })
    }
}

impl FieldTrait for IntMod128BitPrime {
    fn add(&self, rhs: &Self) -> Self {
        let mut value = self.inner.wrapping_add(rhs.inner);
        if value < self.inner || value < rhs.inner || value >= Self::order() {
            // We passed 2^128-159, so add 159 back to get the proper
            // representation as a u128.
            value = value.wrapping_add(159);
        }
        assert!(value < Self::order());
        Self { inner: value }
    }
    fn neg(&self) -> Self {
        let value = if self.inner == 0 {
            0
        } else {
            Self::order() - self.inner
        };
        Self { inner: value }
    }
    fn zero() -> Self {
        return Self { inner: 0 };
    }
    fn mul(&self, _rhs: &Self) -> Self {
        todo!("This is a bit of work");
    }
    fn one() -> Self {
        return Self { inner: 1 };
    }

    fn mul_invert(&self) -> Self {
        todo!("This is a moderate amount of work");
    }
}

#[cfg(test)]
impl Arbitrary for IntMod128BitPrime {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use std::ops::Range;
        let range: Range<u128> = 0..Self::order();
        range
            .prop_map(Self::try_from)
            .prop_map(Result::unwrap)
            .boxed()
    }
}

#[cfg(test)]
mod test_mod128bitprimeu128 {
    use super::test_helpers::*;
    use super::*;

    proptest! {
        #[test]
        fn test_add_associative(a: IntMod128BitPrime, b: IntMod128BitPrime, c: IntMod128BitPrime) {
            run_test_add_associative(a, b, c)?;
        }

        #[test]
        fn test_add_commutative(a: IntMod128BitPrime, b: IntMod128BitPrime) {
            run_test_add_commutative(a, b)?;
        }

        #[test]
        fn test_add_identity(a: IntMod128BitPrime) {
            run_test_add_identity(a)?;
        }

        #[test]
        fn test_add_inverse(a: IntMod128BitPrime) {
            run_test_add_inverse(a)?;
        }

        // Uncomment after implementing mul etc.
        /*
        #[test]
        fn test_mul_associative(a: IntMod128BitPrime, b: IntMod128BitPrime, c: IntMod128BitPrime) {
            run_test_mul_associative(a, b, c)?;
        }

        #[test]
        fn test_mul_commutative(a: IntMod128BitPrime, b: IntMod128BitPrime) {
            run_test_mul_commutative(a, b)?;
        }

        #[test]
        fn test_mul_identity(a: IntMod128BitPrime) {
            run_test_mul_identity(a)?;
        }

        #[test]
        fn test_mul_inverse(a: IntMod128BitPrime) {
            run_test_mul_inverse(a)?;
        }

        #[test]
        fn test_distributive(a: IntMod128BitPrime, b: IntMod128BitPrime, c: IntMod128BitPrime) {
            run_test_distributive(a, b, c)?;
        }
        */
    }
}

/// RUG Integers mod 2^128 - 159
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct IntegerMod128BitPrime {
    inner: Integer,
}

impl IntegerMod128BitPrime {
    const fn order() -> u128 {
        u128::MAX - 158
    }
}

impl TryFrom<u128> for IntegerMod128BitPrime {
    type Error = ();

    fn try_from(value: u128) -> Result<Self, ()> {
        if value >= Self::order() {
            return Err(());
        }
        Ok(Self {
            inner: Integer::from(value),
        })
    }
}

impl TryFrom<Integer> for IntegerMod128BitPrime {
    type Error = ();

    fn try_from(value: Integer) -> Result<Self, ()> {
        if value >= Self::order() {
            return Err(());
        }
        Ok(Self { inner: value })
    }
}

impl FieldTrait for IntegerMod128BitPrime {
    fn add(&self, rhs: &Self) -> Self {
        let value = (self.inner.clone() + rhs.inner.clone()) % Self::order();
        Self {
            inner: value.into(),
        }
    }
    fn neg(&self) -> Self {
        let value = if self.inner == 0 {
            Integer::from(0)
        } else {
            Integer::from(Self::order() - self.inner.clone())
        };
        Self { inner: value }
    }
    fn zero() -> Self {
        Self { inner: 0.into() }
    }
    fn mul(&self, rhs: &Self) -> Self {
        let value = (self.inner.clone() * rhs.inner.clone()) % Self::order();
        Self {
            inner: value.into(),
        }
    }
    fn one() -> Self {
        Self { inner: 1.into() }
    }

    fn mul_invert(&self) -> Self {
        let value: Integer = self.inner.clone();
        let inverse = value
            .invert(&Self::order().into())
            .expect("Expected inverse if self nonzero (prime order field).");
        Self { inner: inverse }
    }
}

impl Sampleable for IntegerMod128BitPrime {
    fn rand_element() -> Self {
        let mut rng = thread_rng();
        rng.gen_range(0u128, Self::order()).try_into().unwrap()
    }

    fn generators(_num: usize, _seed: &crate::prg::aes::AESSeed) -> Vec<Self>
    where
        Self: Sized,
    {
        todo!()
    }
}

#[cfg(test)]
impl Arbitrary for IntegerMod128BitPrime {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use std::ops::Range;
        let range: Range<u128> = 0..Self::order();
        range
            .prop_map(Self::try_from)
            .prop_map(Result::unwrap)
            .boxed()
    }
}

#[cfg(test)]
mod test_mod128bitprime_integer {
    use super::test_helpers::*;
    use super::*;

    proptest! {
        #[test]
        fn test_add_associative(a: IntegerMod128BitPrime, b: IntegerMod128BitPrime, c: IntegerMod128BitPrime) {
            run_test_add_associative(a, b, c)?;
        }

        #[test]
        fn test_add_commutative(a: IntegerMod128BitPrime, b: IntegerMod128BitPrime) {
            run_test_add_commutative(a, b)?;
        }

        #[test]
        fn test_add_identity(a: IntegerMod128BitPrime) {
            run_test_add_identity(a)?;
        }

        #[test]
        fn test_add_inverse(a: IntegerMod128BitPrime) {
            run_test_add_inverse(a)?;
        }

        #[test]
        fn test_mul_associative(a: IntegerMod128BitPrime, b: IntegerMod128BitPrime, c: IntegerMod128BitPrime) {
            run_test_mul_associative(a, b, c)?;
        }

        #[test]
        fn test_mul_commutative(a: IntegerMod128BitPrime, b: IntegerMod128BitPrime) {
            run_test_mul_commutative(a, b)?;
        }

        #[test]
        fn test_mul_identity(a: IntegerMod128BitPrime) {
            run_test_mul_identity(a)?;
        }

        #[test]
        fn test_mul_inverse(a: IntegerMod128BitPrime) {
            run_test_mul_inverse(a)?;
        }

        #[test]
        fn test_distributive(a: IntegerMod128BitPrime, b: IntegerMod128BitPrime, c: IntegerMod128BitPrime) {
            run_test_distributive(a, b, c)?;
        }
    }
}

use crate::group::Group;
use jubjub::Fr as Jubjub;
impl FieldTrait for GroupElement {
    fn add(&self, rhs: &Self) -> Self {
        self.op(rhs)
    }

    fn neg(&self) -> Self {
        self.invert()
    }

    fn zero() -> Self {
        Jubjub::zero().into()
    }

    fn mul(&self, rhs: &Self) -> Self {
        self.inner.mul(&rhs.inner).into()
    }

    fn mul_invert(&self) -> Self {
        self.inner.invert().unwrap().into()
    }

    fn one() -> Self {
        Jubjub::one().into()
    }
}

#[cfg(test)]
mod test_jubjub {
    use super::test_helpers::*;
    use super::*;

    proptest! {
        #[test]
        fn test_add_associative(a: GroupElement, b: GroupElement, c: GroupElement) {
            run_test_add_associative(a, b, c)?;
        }

        #[test]
        fn test_add_commutative(a: GroupElement, b: GroupElement) {
            run_test_add_commutative(a, b)?;
        }

        #[test]
        fn test_add_identity(a: GroupElement) {
            run_test_add_identity(a)?;
        }

        #[test]
        fn test_add_inverse(a: GroupElement) {
            run_test_add_inverse(a)?;
        }

        #[test]
        fn test_mul_associative(a: GroupElement, b: GroupElement, c: GroupElement) {
            run_test_mul_associative(a, b, c)?;
        }

        #[test]
        fn test_mul_commutative(a: GroupElement, b: GroupElement) {
            run_test_mul_commutative(a, b)?;
        }

        #[test]
        fn test_mul_identity(a: GroupElement) {
            run_test_mul_identity(a)?;
        }

        #[test]
        fn test_mul_inverse(a: GroupElement) {
            run_test_mul_inverse(a)?;
        }

        #[test]
        fn test_distributive(a: GroupElement, b: GroupElement, c: GroupElement) {
            run_test_distributive(a, b, c)?;
        }
    }
}

// NOTE: can't use From/Into due to Rust orphaning rules. Define an extension trait?
// TODO(zjn): more efficient data format?
#[cfg(feature = "proto")]
fn parse_integer(data: &str) -> Integer {
    Integer::parse(data).unwrap().into()
}

#[cfg(feature = "proto")]
fn emit_integer(value: &Integer) -> String {
    value.to_string()
}

#[cfg(feature = "proto")]
impl From<proto::Integer> for IntegerMod128BitPrime {
    fn from(msg: proto::Integer) -> Self {
        parse_integer(msg.data.as_ref()).try_into().unwrap()
    }
}

#[cfg(feature = "proto")]
impl Into<proto::Integer> for IntegerMod128BitPrime {
    fn into(self) -> proto::Integer {
        proto::Integer {
            data: emit_integer(&self.inner),
        }
    }
}

//     pub fn element_from_bytes(&self, bytes: &Bytes) -> FieldElement {
//         let val = Integer::from_digits(bytes.as_ref(), BYTE_ORDER);
//         self.new_element(val)
//     }

// impl Into<Bytes> for FieldElement {
//     fn into(self) -> Bytes {
//         Bytes::from(self.value.to_digits(Order::LsfLe))
//     }
// }
//
// #[cfg(feature = "proto")]
// impl Into<proto::Integer> for FieldElement {
//     fn into(self) -> proto::Integer {
//         proto::Integer {
//             data: emit_integer(&self.value),
//         }
//     }
// }

/// Test helpers
#[cfg(any(test, feature = "testing"))]
pub mod testing {
    use super::*;

    pub fn integers() -> impl Strategy<Value = Integer> {
        (0..1000).prop_map(Integer::from)
    }

    pub fn prime_integers() -> impl Strategy<Value = Integer> {
        integers().prop_map(Integer::next_prime)
    }
}

#[cfg(test)]
pub mod tests {
    // use super::testing::*;
    // use super::*;
    // use std::collections::HashSet;
    // use std::iter::repeat_with;

    // proptest! {
    //     #[test]
    //     fn run_test_field_rand_not_deterministic() {
    //     }
    // }
    /*

    proptest! {

    #[test]
    fn test_field_element_bytes_rt(element: FieldElement) {
        prop_assert_eq!(
            element.field.element_from_bytes(&element.clone().into()),
            element
        );
      }
    }
    */
}
