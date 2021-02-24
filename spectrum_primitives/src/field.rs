//! Spectrum implementation.
use crate::bytes::Bytes;

use jubjub::Fr as Jubjub;
use rug::{integer::IsPrime, integer::Order, rand::RandState, Integer};
use serde::{Deserialize, Serialize};

use std::cmp::Ordering;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::ops;
use std::sync::Arc;

#[cfg(feature = "proto")]
use crate::proto;

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

const BYTE_ORDER: Order = Order::LsfLe;

trait FieldTrait: Eq + PartialEq {
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
}

#[cfg(test)]
impl Arbitrary for IntMod5 {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use std::ops::Range;
        let range: Range<u8> = 0..4;
        range
            .prop_map(Self::try_from)
            .prop_map(Result::unwrap)
            .boxed()
    }
}

#[cfg(test)]
mod test_mod5 {
    use super::test_helpers::*;
    use super::*;

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
    }
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
        let range: Range<u128> = 0..(Self::order() - 1);
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
struct IntegerMod128BitPrime {
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
        Self {
            inner: self
                .inner
                .clone()
                .invert(&Self::order().into())
                .expect("Expected inverse if self nonzero (prime order field)."),
        }
    }
}

#[cfg(test)]
impl Arbitrary for IntegerMod128BitPrime {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use std::ops::Range;
        let range: Range<u128> = 0..(Self::order() - 1);
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

impl FieldTrait for Jubjub {
    fn add(&self, rhs: &Self) -> Self {
        self.add(rhs)
    }

    fn neg(&self) -> Self {
        self.neg()
    }

    fn zero() -> Self {
        Jubjub::zero()
    }

    fn mul(&self, rhs: &Self) -> Self {
        self.mul(rhs)
    }

    fn mul_invert(&self) -> Self {
        self.invert().unwrap()
    }

    fn one() -> Self {
        Jubjub::one()
    }
}

#[cfg(test)]
mod test_jubjub {
    use super::test_helpers::*;
    use super::*;
    use crate::group::jubjubs;

    proptest! {
        #[test]
        fn test_add_associative(a in jubjubs(), b in jubjubs(), c in jubjubs()) {
            run_test_add_associative(a, b, c)?;
        }

        #[test]
        fn test_add_commutative(a in jubjubs(), b in jubjubs()) {
            run_test_add_commutative(a, b)?;
        }

        #[test]
        fn test_add_identity(a in jubjubs()) {
            run_test_add_identity(a)?;
        }

        #[test]
        fn test_add_inverse(a in jubjubs()) {
            run_test_add_inverse(a)?;
        }

        #[test]
        fn test_mul_associative(a in jubjubs(), b in jubjubs(), c in jubjubs()) {
            run_test_mul_associative(a, b, c)?;
        }

        #[test]
        fn test_mul_commutative(a in jubjubs(), b in jubjubs()) {
            run_test_mul_commutative(a, b)?;
        }

        #[test]
        fn test_mul_identity(a in jubjubs()) {
            run_test_mul_identity(a)?;
        }

        #[test]
        fn test_mul_inverse(a in jubjubs()) {
            run_test_mul_inverse(a)?;
        }

        #[test]
        fn test_distributive(a in jubjubs(), b in jubjubs(), c in jubjubs()) {
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

/// prime order field
#[derive(Clone, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub struct Field {
    order: Arc<Integer>,
}

impl From<Integer> for Field {
    fn from(value: Integer) -> Field {
        Field::new(value)
    }
}

#[cfg(feature = "proto")]
impl From<proto::Integer> for Field {
    fn from(msg: proto::Integer) -> Field {
        parse_integer(msg.data.as_ref()).into()
    }
}

#[cfg(feature = "proto")]
impl Into<proto::Integer> for Field {
    fn into(self) -> proto::Integer {
        proto::Integer {
            data: emit_integer(&self.order),
        }
    }
}

/// element in a prime order field
#[derive(Clone, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub struct FieldElement {
    value: Integer,
    field: Field,
}

impl Field {
    /// generate new field of prime order
    /// order must be a prime
    pub fn new(order: Integer) -> Field {
        // probability of failure is negligible in k, suggested to set k=15
        // which is the default used by Rust https://docs.rs/rug/1.6.0/rug/struct.Integer.html
        if order.is_probably_prime(15) == IsPrime::No {
            panic!("field must have prime order!");
        }

        Field {
            order: Arc::new(order),
        }
    }

    pub fn zero(&self) -> FieldElement {
        FieldElement {
            value: 0.into(),
            field: self.clone(),
        }
    }

    pub fn new_element(&self, value: Integer) -> FieldElement {
        FieldElement::new(value, self.clone())
    }

    // generates a new random field element
    pub fn rand_element(&self, rng: &mut RandState) -> FieldElement {
        FieldElement {
            value: self.order.random_below_ref(rng).into(),
            field: self.clone(),
        }
    }

    #[cfg(feature = "proto")]
    pub fn from_proto(&self, msg: proto::Integer) -> FieldElement {
        FieldElement::new(parse_integer(msg.data.as_ref()), self.clone())
    }

    pub fn element_from_bytes(&self, bytes: &Bytes) -> FieldElement {
        let val = Integer::from_digits(bytes.as_ref(), BYTE_ORDER);
        self.new_element(val)
    }
}

impl FieldElement {
    // TODO: move reduce_modulo to Field::new_element and remove
    /// generates a new field element; value mod field.order
    pub fn new(v: Integer, field: Field) -> FieldElement {
        FieldElement {
            value: reduce_modulo(v, &field.order),
            field,
        }
    }

    pub fn field(&self) -> Field {
        self.field.clone()
    }

    pub fn get_value(&self) -> Integer {
        self.value.clone()
    }
}

impl Into<Bytes> for FieldElement {
    fn into(self) -> Bytes {
        Bytes::from(self.value.to_digits(Order::LsfLe))
    }
}

#[cfg(feature = "proto")]
impl Into<proto::Integer> for FieldElement {
    fn into(self) -> proto::Integer {
        proto::Integer {
            data: emit_integer(&self.value),
        }
    }
}

// perform modulo reducation after a field operation.
// note: different from % given that reduce_modulo compares
// to zero rather than just take the remainder.
fn reduce_modulo(v: Integer, order: &Integer) -> Integer {
    if v.cmp0() == Ordering::Less {
        (order.clone() - (-v % order)) % order
    } else {
        v % order
    }
}

/// override + operation: want result.value = element1.value  + element2.value  mod field.order
impl ops::Add<FieldElement> for FieldElement {
    type Output = FieldElement;

    #[allow(clippy::suspicious_arithmetic_impl)]
    fn add(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);

        let mut value = self.value + other.value;

        // perform modulo reduction after adding
        if (&value) >= self.field.order.as_ref() {
            value -= self.field.order.as_ref();
        }

        FieldElement::new(value, other.field)
    }
}

impl std::iter::Sum<FieldElement> for FieldElement {
    fn sum<I>(iter: I) -> FieldElement
    where
        I: Iterator<Item = FieldElement>,
    {
        let mut iter = iter.peekable();
        let zero = iter
            .peek()
            .expect("Cannot sum empty iterator.")
            .field()
            .zero();
        iter.fold(zero, |a, b| a + b)
    }
}

/// override - operation: want result.vvalue = element1.value  + (-element2.value) mod field.order
impl ops::Sub<FieldElement> for FieldElement {
    type Output = FieldElement;

    #[allow(clippy::suspicious_arithmetic_impl)]
    fn sub(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);
        let mut value = self.value - other.value;

        // perform modulo reduction after subtracting
        if value < 0 {
            value += self.field.order.as_ref();
        }

        FieldElement::new(value, other.field)
    }
}

/// override * operation: want result.value = element1.value * element2.value mod field.order
impl ops::Mul<FieldElement> for FieldElement {
    type Output = FieldElement;

    fn mul(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);
        FieldElement::new(
            reduce_modulo(self.value * &other.value, &other.field.order),
            other.field,
        )
    }
}

/// override * operation: want result.value = element1.value * element2.value mod field.order
impl ops::Mul<u8> for FieldElement {
    type Output = FieldElement;

    fn mul(self, scalar: u8) -> FieldElement {
        FieldElement::new(
            reduce_modulo(self.value * scalar, &self.field.order),
            self.field,
        )
    }
}

/// override negation of field element
impl ops::Neg for FieldElement {
    type Output = FieldElement;

    fn neg(self) -> FieldElement {
        FieldElement::new(reduce_modulo(-self.value, &self.field.order), self.field)
    }
}

/// override += operation
impl ops::AddAssign<FieldElement> for FieldElement {
    fn add_assign(&mut self, other: FieldElement) {
        assert_eq!(self.field, other.field);
        *self = Self {
            value: reduce_modulo(self.value.clone() + other.value, &other.field.order),
            field: other.field,
        };
    }
}

impl ops::SubAssign<FieldElement> for FieldElement {
    fn sub_assign(&mut self, other: FieldElement) {
        assert_eq!(self.field, other.field);
        *self = Self {
            value: reduce_modulo(self.value.clone() - other.value, &other.field.order),
            field: other.field,
        };
    }
}

/// Test helpers
#[cfg(any(test, feature = "testing"))]
pub mod testing {
    use super::*;

    // Pair of field elements *in the same field*
    pub fn field_element_pairs() -> impl Strategy<Value = (FieldElement, FieldElement)> {
        any::<Field>().prop_flat_map(|field| {
            (
                any_with::<FieldElement>(Some(field.clone())),
                any_with::<FieldElement>(Some(field)),
            )
        })
    }

    pub fn integers() -> impl Strategy<Value = Integer> {
        (0..1000).prop_map(Integer::from)
    }

    pub fn prime_integers() -> impl Strategy<Value = Integer> {
        integers().prop_map(Integer::next_prime)
    }
}

#[cfg(any(test, feature = "testing"))]
impl Arbitrary for Field {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        testing::prime_integers().prop_map(Field::from).boxed()
    }
}

#[cfg(any(test, feature = "testing"))]
impl Arbitrary for FieldElement {
    type Parameters = Option<Field>;
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(field: Self::Parameters) -> Self::Strategy {
        match field {
            Some(field) => testing::integers()
                .prop_map(move |value| field.new_element(value))
                .boxed(),
            None => (testing::integers(), any::<Field>())
                .prop_map(|(value, field)| field.new_element(value))
                .boxed(),
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::testing::*;
    use super::*;
    use std::collections::HashSet;
    use std::iter::repeat_with;

    // Pair of field elements *in the same field*
    fn field_element_triples() -> impl Strategy<Value = (FieldElement, FieldElement, FieldElement)>
    {
        any::<Field>().prop_flat_map(|field| {
            (
                any_with::<FieldElement>(Some(field.clone())),
                any_with::<FieldElement>(Some(field.clone())),
                any_with::<FieldElement>(Some(field)),
            )
        })
    }

    // Several field elements, *all in the same field*
    pub fn field_element_vecs(num: usize) -> impl Strategy<Value = Vec<FieldElement>> {
        any::<Field>().prop_flat_map(move |field| {
            prop::collection::vec(integers().prop_map(move |v| field.new_element(v)), num)
        })
    }

    proptest! {
        #[test]
        fn test_field_rand_element_not_deterministic(field in any::<Field>()) {
            let mut rng = RandState::new();
            let elements: HashSet<_> = repeat_with(|| field.rand_element(&mut rng))
                .take(10)
                .collect();
            assert!(
                elements.len() > 1,
                "Many random elements should not all be the same."
            );
        }
    }

    proptest! {

    #[test]
    fn test_field_element_bytes_rt(element: FieldElement) {
        prop_assert_eq!(
            element.field.element_from_bytes(&element.clone().into()),
            element
        );
      }
    }

    proptest! {

        #[test]
        fn test_add_commutative((x, y) in field_element_pairs()) {
            assert_eq!(x.clone() + y.clone(), y + x);
        }
        #[test]
        fn test_add_associative((x, y, z) in field_element_triples()) {
            assert_eq!((x.clone() + y.clone()) + z.clone(), x + (y + z));
        }

        #[test]
        fn test_add_sub_inverses((x, y) in field_element_pairs()) {
            assert_eq!(x.clone() - y.clone(), x + (-y));
        }

        #[test]
        fn test_add_inverse(x in any::<FieldElement>()) {
            assert_eq!(x.field().zero(), x.clone() + (-x));
        }

        #[test]
        fn test_mul_commutative((x, y) in field_element_pairs()) {
            assert_eq!(x.clone() * y.clone(), y * x);
        }

        #[test]
        fn test_mul_associative((x, y, z) in field_element_triples()) {
            assert_eq!((x.clone() * y.clone()) * z.clone(), x * (y * z));
        }

        #[test]
        fn test_distributive((x, y, z) in field_element_triples()) {
            assert_eq!(x.clone() * (y.clone() + z.clone()), (x.clone() * y) + (x * z));
        }

        #[test]
        fn test_negative(field:Field, x in integers(), y in integers()) {
            let expected = FieldElement::new(x.clone() - y.clone(), field.clone());
            let actual = FieldElement::new(x, field.clone()) - FieldElement::new(y, field);
            assert_eq!(actual, expected);
        }


        #[test]
        #[should_panic]
        fn test_add_in_different_fields_fails(a: FieldElement, b: FieldElement) {
            prop_assume!(a.field.order != b.field.order, "Fields should not be equal");
            a + b
        }


        #[test]
        #[should_panic]
        fn test_prod_in_different_fields_fails(a: FieldElement, b: FieldElement) {
            prop_assume!(a.field.order != b.field.order, "Fields should not be equal");
            a * b
        }


        #[test]
        fn test_equality((a, b) in field_element_pairs()) {
            let eq = a == b && a.value == b.value && a.field.order == b.field.order;
            let neq = a != b && (a.value != b.value || a.field.order != b.field.order);
            assert!(neq || eq);
        }

    }

    #[test]
    #[should_panic]
    fn test_field_must_be_prime() {
        Field::new(Integer::from(4));
    }
}
