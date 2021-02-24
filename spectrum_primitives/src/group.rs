//! Spectrum implementation.
use crate::bytes::Bytes;
use crate::prg::{aes::AESSeed, aes::AESPRG, PRG};

use jubjub::Fr as Jubjub; // elliptic curve field
use rand::prelude::*;
use rug::{integer::Order, Integer};
use serde::{de, ser::Serializer, Deserialize, Serialize};

use std::cmp::Ordering;
use std::convert::{From, TryFrom, TryInto};
use std::fmt::Debug;
use std::hash::Hash;
use std::hash::Hasher;
use std::ops;

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

const BYTE_ORDER: Order = Order::LsfLe;

// see jubjub::Fr for details
// PR to expose this as public within the library:
// https://github.com/zkcrypto/jubjub/pull/34
const JUBJUB_MODULUS: [u64; 4] = [
    0xd097_0e5e_d6f7_2cb7_u64,
    0xa668_2093_ccc8_1082_u64,
    0x0667_3b01_0134_3b00_u64,
    0x0e7d_b4ea_6533_afa9_u64,
];

// size of group elements in jubjbu
const JUBJUB_MODULUS_BYTES: usize = 32;

/// A *commutative* group
pub trait Group: Eq {
    fn order() -> Integer;
    fn order_size_in_bytes() -> usize {
        Self::order().significant_digits::<u8>()
    }
    fn identity() -> Self;
    fn op(&self, rhs: &Self) -> Self;
    fn invert(&self) -> Self;
    fn pow(&self, pow: &Integer) -> Self;
}

pub trait SampleableGroup: Group {
    /// generates a new random group element
    fn rand_element() -> Self;
    /// Generate the given number of group generators deterministically from the given seed.
    fn generators(num: usize, seed: &AESSeed) -> Vec<Self>
    where
        Self: Sized;
}

#[cfg(test)]
pub mod tests_common {
    use super::*;

    pub(super) fn run_test_associative<G>(a: G, b: G, c: G) -> Result<(), TestCaseError>
    where
        G: Group + Debug,
    {
        prop_assert_eq!(a.op(&b).op(&c), a.op(&b.op(&c)));
        Ok(())
    }

    pub(super) fn run_test_commutative<G>(a: G, b: G) -> Result<(), TestCaseError>
    where
        G: Group + Debug,
    {
        prop_assert_eq!(a.op(&b), b.op(&a));
        Ok(())
    }

    pub(super) fn run_test_identity<G>(a: G) -> Result<(), TestCaseError>
    where
        G: Group + Debug,
    {
        prop_assert_eq!(a.op(&G::identity()), a);
        Ok(())
    }

    pub(super) fn run_test_inverse<G>(a: G) -> Result<(), TestCaseError>
    where
        G: Group + Debug,
    {
        prop_assert_eq!(a.op(&a.invert()), G::identity());
        Ok(())
    }

    // TODO: there are a bunch of other good exponent things to check
    // exp *should* be an integer but the way we're checking is a loop so don't want it too big
    pub(super) fn run_test_exponent_definition<G>(base: G, exp: u16) -> Result<(), TestCaseError>
    where
        G: Group + Debug,
    {
        let actual = base.pow(&exp.into());
        let mut expected = G::identity();
        for _ in 0..exp {
            expected = expected.op(&base)
        }
        prop_assert_eq!(actual, expected);
        Ok(())
    }
}

#[cfg(test)]
pub mod tests_u8_group {
    use super::*;

    /// Just for testing
    impl Group for u8 {
        fn order() -> Integer {
            Integer::from(256)
        }

        fn identity() -> Self {
            0
        }

        fn op(&self, rhs: &Self) -> Self {
            (((*self as u16) + (*rhs as u16)) % 256).try_into().unwrap()
        }

        fn invert(&self) -> Self {
            if *self == 0 {
                0
            } else {
                u8::MAX - self + 1
            }
        }

        fn pow(&self, pow: &Integer) -> Self {
            // where group op is addition, exponentiation is multiplication
            Integer::from(pow * self).to_u8_wrapping()
        }
    }

    proptest! {
        #[test]
        fn test_associative(a: u8, b: u8, c: u8) {
            tests_common::run_test_associative(a, b, c)?;
        }

        #[test]
        fn test_commutative(a: u8, b: u8) {
            tests_common::run_test_commutative(a, b)?;
        }


        #[test]
        fn test_identity(a: u8) {
            tests_common::run_test_identity(a)?;
        }

        #[test]
        fn test_inverse(a: u8) {
            tests_common::run_test_inverse(a)?;
        }

        #[test]
        fn test_exponent_definition(base: u8, exp: u16) {
            tests_common::run_test_exponent_definition(base, exp)?;
        }
    }
}

// there's a lot of mess around conversion to/from bytes
// probably insecure..look into using e.g. curve25519
fn serialize_field_element<S>(x: &Jubjub, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_bytes(&x.to_bytes())
}

fn deserialize_field_element<'de, D>(deserializer: D) -> Result<Jubjub, D::Error>
where
    D: de::Deserializer<'de>,
{
    let bytes: Vec<u8> = de::Deserialize::deserialize(deserializer)?;
    let bytes: &[u8] = bytes.as_ref();
    let bytes: &[u8; 32] = bytes.try_into().unwrap();
    Ok(Jubjub::from_bytes(bytes).unwrap())
}

/// element within a group
#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct GroupElement {
    #[serde(
        serialize_with = "serialize_field_element",
        deserialize_with = "deserialize_field_element"
    )]
    inner: Jubjub,
}

impl SampleableGroup for GroupElement {
    /// identity element in the elliptic curve field

    /// generates a new random group element
    fn rand_element() -> GroupElement {
        // generate enough random bytes to create a random element in the group
        let mut bytes = vec![0; JUBJUB_MODULUS_BYTES - 1];
        thread_rng().fill_bytes(&mut bytes);
        GroupElement::try_from(Bytes::from(bytes))
            .expect("chunk size chosen s.t. always valid element")
    }

    /// generates a set of field elements in the elliptic curve field
    /// which are generators for the group (given that the group is of prime order)
    /// takes as input a random seed which deterministically generates [num] field elements
    fn generators(num: usize, seed: &AESSeed) -> Vec<GroupElement> {
        let prg = AESPRG::new(16, (JUBJUB_MODULUS_BYTES - 1) * num);
        let rand_bytes: Vec<u8> = prg.eval(seed).into();

        //TODO: maybe use itertools::Itertools chunks?
        (0..num)
            .map(|i| {
                let mut chunk = rand_bytes
                    [i * (JUBJUB_MODULUS_BYTES - 1)..(i + 1) * (JUBJUB_MODULUS_BYTES - 1)]
                    .to_vec();
                chunk.push(0);
                GroupElement::try_from(Bytes::from(chunk))
                    .expect("chunk size chosen s.t. always valid element")
            })
            .collect()
    }
}

impl From<Jubjub> for GroupElement {
    fn from(inner: Jubjub) -> GroupElement {
        GroupElement { inner }
    }
}

impl From<&Integer> for GroupElement {
    fn from(value: &Integer) -> GroupElement {
        let reduced = if value.cmp0() == Ordering::Less {
            GroupElement::order() - (Integer::from(-value) % Self::order())
        } else {
            value % Self::order()
        };

        let mut digits: [u8; JUBJUB_MODULUS_BYTES] = [0x0u8; JUBJUB_MODULUS_BYTES];
        reduced.write_digits(&mut digits, BYTE_ORDER);
        Jubjub::from_bytes(&digits).unwrap().into()
    }
}

impl Group for GroupElement {
    fn order() -> Integer {
        // see JubJub elliptic curve modulus
        Integer::from_digits(&JUBJUB_MODULUS, BYTE_ORDER)
    }

    fn identity() -> Self {
        Jubjub::zero().into()
    }
    fn op(&self, rhs: &Self) -> Self {
        self.inner.add(&rhs.inner).into()
    }

    fn invert(&self) -> Self {
        self.inner.neg().into()
    }

    fn pow(&self, pow: &Integer) -> Self {
        // in EC group operation is addition, so exponentiation = multiplying
        (self.inner * GroupElement::from(pow).inner).into()
    }
}

#[cfg(any(test, feature = "testing"))]
pub(crate) fn jubjubs() -> impl Strategy<Value = Jubjub> {
    proptest::collection::vec(any::<u8>(), 64)
        .prop_map(|v| Jubjub::from_bytes_wide(v.as_slice().try_into().unwrap()))
}

#[cfg(any(test, feature = "testing"))]
impl Arbitrary for GroupElement {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        jubjubs().prop_map(GroupElement::from).boxed()
    }
}

impl Into<Bytes> for GroupElement {
    fn into(self) -> Bytes {
        Bytes::from(self.inner.to_bytes().to_vec())
    }
}

impl TryFrom<Bytes> for GroupElement {
    type Error = &'static str;

    fn try_from(bytes: Bytes) -> Result<Self, Self::Error> {
        assert_eq!(bytes.len(), JUBJUB_MODULUS_BYTES, "uh oh");
        let mut bytes_arr: [u8; JUBJUB_MODULUS_BYTES] = [0; JUBJUB_MODULUS_BYTES];
        bytes_arr.copy_from_slice(bytes.as_ref());
        let result = Jubjub::from_bytes(&bytes_arr);
        if result.is_some().into() {
            Ok(result.unwrap().into())
        } else {
            Err("converting from bytes failed")
        }
    }
}

impl ops::Mul<GroupElement> for GroupElement {
    type Output = GroupElement;

    fn mul(self, rhs: GroupElement) -> GroupElement {
        self.inner.add(&rhs.inner).into()
    }
}

impl ops::Mul<&GroupElement> for GroupElement {
    type Output = GroupElement;

    fn mul(self, rhs: &GroupElement) -> GroupElement {
        self.inner.add(&rhs.inner).into()
    }
}

impl<'a, 'b> ops::Mul<&'b GroupElement> for &'a GroupElement {
    type Output = GroupElement;

    fn mul(self, rhs: &'b GroupElement) -> GroupElement {
        self.inner.add(&rhs.inner).into()
    }
}

impl ops::MulAssign<&GroupElement> for GroupElement {
    fn mul_assign(&mut self, rhs: &GroupElement) {
        self.inner = self.inner.add(&rhs.inner);
    }
}

impl ops::MulAssign<GroupElement> for GroupElement {
    fn mul_assign(&mut self, rhs: GroupElement) {
        self.inner = self.inner.add(&rhs.inner);
    }
}

impl Hash for GroupElement {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.inner.to_bytes().hash(state);
    }
}

/// Test helpers
#[cfg(any(test))]
mod testing {
    use super::*;

    // need to generate 512-bit integers to ensure all operations
    // "wrap around" the group order during testing
    pub(super) fn integer_512_bits() -> impl Strategy<Value = Integer> {
        any_with::<Bytes>(JUBJUB_MODULUS_BYTES.into())
            .prop_map(|bytes| Integer::from_digits(&bytes.as_ref(), BYTE_ORDER))
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::bytes::Bytes;

    use rug::integer::IsPrime;

    use std::ops::Range;

    const NUM_GROUP_GENERATORS: Range<usize> = 1..500;

    #[test]
    fn group_is_of_prime_order() {
        assert_ne!(GroupElement::order().is_probably_prime(15), IsPrime::No)
    }

    fn valid_group_bytes() -> impl Strategy<Value = Bytes> {
        any_with::<Bytes>(32.into()).prop_map(|data| {
            let mut data: Vec<u8> = data.into();
            data[31] &= 0x0d;
            data.into()
        })
    }

    proptest! {
         #[test]
         fn test_associative(a: GroupElement, b: GroupElement, c: GroupElement) {
             tests_common::run_test_associative(a, b, c)?;
         }

         #[test]
         fn test_commutative(a: GroupElement, b: GroupElement) {
             tests_common::run_test_commutative(a, b)?;
         }


         #[test]
         fn test_identity(a: GroupElement) {
             tests_common::run_test_identity(a)?;
         }

         #[test]
         fn test_inverse(a: GroupElement) {
             tests_common::run_test_inverse(a)?;
         }

        #[test]
        fn test_pow_prod(element: GroupElement, a in integer_512_bits(), b in integer_512_bits()) {
            let prod = a.clone() * b.clone();
            let expected = prod % GroupElement::order();
            assert_eq!(element.pow(&a).pow(&b), element.pow(&expected))
        }

        #[test]
        fn test_pow_negative(element: GroupElement, a in integer_512_bits()) {
            let negative = -(a.clone() % GroupElement::order());
            let expected = GroupElement::order() - (a % GroupElement::order());
            assert_eq!(element.pow(&negative), element.pow(&expected))
        }

        #[test]
        fn test_sums_in_exponent(element: GroupElement, a in integer_512_bits(), b in integer_512_bits()) {
            let expected = a.clone() + b.clone() % GroupElement::order();
            assert_eq!(element.pow(&a) * element.pow(&b), element.pow(&expected))
        }

        #[test]
        fn test_generators_deterministic(
            num in NUM_GROUP_GENERATORS,
            seed: AESSeed) {
            assert_eq!(GroupElement::generators(num, &seed), GroupElement::generators(num, &seed));
        }

        #[test]
        fn test_generators_different_seeds_different_generators(
            num in NUM_GROUP_GENERATORS,
            seed1: AESSeed,
            seed2: AESSeed
        ) {
            prop_assume!(seed1 != seed2, "Different generators only for different seeds");
            assert_ne!(GroupElement::generators(num, &seed1), GroupElement::generators(num, &seed2));
        }

        #[test]
        fn test_element_bytes_roundtrip(x: GroupElement) {
            prop_assert_eq!(Ok(x.clone()), GroupElement::try_from(Into::<Bytes>::into(x)));
        }

        #[test]
        fn test_bytes_element_roundtrip(before in valid_group_bytes()) {
            prop_assert_eq!(
                before.clone(),
                GroupElement::try_from(before).unwrap().into()
            );
        }

        #[test]
        fn test_element_serialize_roundtrip(x: GroupElement) {
            let json_string = serde_json::to_string(&x).unwrap();
            assert_eq!(
                serde_json::from_str::<GroupElement>(&json_string).unwrap(),
                x
            );
        }
    }
}
