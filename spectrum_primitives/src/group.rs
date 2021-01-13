//! Spectrum implementation.
use crate::bytes::Bytes;
use crate::prg::{aes::AESSeed, aes::AESPRG, PRG};

use jubjub::Fr as ECFieldElement; // elliptic curve field
use rand::prelude::*;
use rug::{integer::Order, Integer};
use serde::{de, ser::Serializer, Deserialize, Serialize};

use std::cmp::Ordering;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::hash::Hash;
use std::hash::Hasher;
use std::ops;

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

/// mathematical group object
// there's a lot of mess around conversion to/from bytes
// probably insecure..look into using e.g. curve25519
#[derive(Default, Clone, Eq, PartialEq, Debug)]
pub struct Group(ECFieldElement); // generator for the multiplicative group

fn serialize_field_element<S>(x: &ECFieldElement, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_bytes(&x.to_bytes())
}

fn deserialize_field_element<'de, D>(deserializer: D) -> Result<ECFieldElement, D::Error>
where
    D: de::Deserializer<'de>,
{
    use std::convert::TryInto;
    let bytes: Vec<u8> = de::Deserialize::deserialize(deserializer)?;
    let bytes: &[u8] = bytes.as_ref();
    let bytes: &[u8; 32] = bytes.try_into().unwrap();
    Ok(ECFieldElement::from_bytes(bytes).unwrap())
}

/// element within a group
#[derive(Clone, Eq, Debug, Serialize, Deserialize)]
pub struct GroupElement(
    #[serde(
        serialize_with = "serialize_field_element",
        deserialize_with = "deserialize_field_element"
    )]
    ECFieldElement,
);

impl Group {
    /// identity element in the elliptic curve field
    pub fn identity() -> GroupElement {
        GroupElement(ECFieldElement::zero())
    }

    /// creates a new group element from an integer
    pub fn new_element(value: &Integer) -> GroupElement {
        let reduced = if value.cmp0() == Ordering::Less {
            Group::order() - (Integer::from(-value) % Self::order())
        } else {
            value % Self::order()
        };

        let mut digits: [u8; JUBJUB_MODULUS_BYTES] = [0x0u8; JUBJUB_MODULUS_BYTES];
        reduced.write_digits(&mut digits, BYTE_ORDER);
        GroupElement(ECFieldElement::from_bytes(&digits).unwrap())
    }

    /// generates a new random group element
    pub fn rand_element() -> GroupElement {
        // generate enough random bytes to create a random element in the group
        let mut bytes = vec![0; JUBJUB_MODULUS_BYTES - 1];
        thread_rng().fill_bytes(&mut bytes);
        GroupElement::try_from(Bytes::from(bytes))
            .expect("chunk size chosen s.t. always valid element")
    }

    /// generates a set of field elements in the elliptic curve field
    /// which are generators for the group (given that the group is of prime order)
    /// takes as input a random seed which deterministically generates [num] field elements
    pub fn generators(num: usize, seed: &AESSeed) -> Vec<GroupElement> {
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

    pub fn order() -> Integer {
        // see JubJub elliptic curve modulus
        Integer::from_digits(&JUBJUB_MODULUS, BYTE_ORDER)
    }

    pub fn order_size_in_bytes() -> usize {
        JUBJUB_MODULUS_BYTES // size of the group elements
    }
}

impl GroupElement {
    pub fn pow(&self, pow: &Integer) -> GroupElement {
        // in EC group operation is addition, so exponentiation = multiplying
        GroupElement(self.0 * Group::new_element(pow).0)
    }
}

impl Into<Bytes> for GroupElement {
    fn into(self) -> Bytes {
        Bytes::from(self.0.to_bytes().to_vec())
    }
}

impl TryFrom<Bytes> for GroupElement {
    type Error = &'static str;

    fn try_from(bytes: Bytes) -> Result<Self, Self::Error> {
        assert_eq!(bytes.len(), JUBJUB_MODULUS_BYTES, "uh oh");
        let mut bytes_arr: [u8; JUBJUB_MODULUS_BYTES] = [0; JUBJUB_MODULUS_BYTES];
        bytes_arr.copy_from_slice(bytes.as_ref());
        let result = ECFieldElement::from_bytes(&bytes_arr);
        if result.is_some().into() {
            Ok(GroupElement(result.unwrap()))
        } else {
            Err("convering to bytes failed")
        }
    }
}

impl ops::Mul<GroupElement> for GroupElement {
    type Output = GroupElement;

    fn mul(self, rhs: GroupElement) -> GroupElement {
        GroupElement(self.0.add(&rhs.0))
    }
}

impl ops::Mul<&GroupElement> for GroupElement {
    type Output = GroupElement;

    fn mul(self, rhs: &GroupElement) -> GroupElement {
        GroupElement(self.0.add(&rhs.0))
    }
}

impl<'a, 'b> ops::Mul<&'b GroupElement> for &'a GroupElement {
    type Output = GroupElement;

    fn mul(self, rhs: &'b GroupElement) -> GroupElement {
        GroupElement(self.0.add(&rhs.0))
    }
}

impl ops::MulAssign<&GroupElement> for GroupElement {
    fn mul_assign(&mut self, rhs: &GroupElement) {
        self.0 = self.0.add(&rhs.0);
    }
}

impl ops::MulAssign<GroupElement> for GroupElement {
    fn mul_assign(&mut self, rhs: GroupElement) {
        self.0 = self.0.add(&rhs.0);
    }
}

impl Hash for GroupElement {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.0.to_bytes().hash(state);
    }
}

// need to implement PartialEq when implementing Hash...
impl PartialEq for GroupElement {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::Bytes;

    use proptest::prelude::*;
    use rug::integer::IsPrime;

    use std::ops::Range;

    const NUM_GROUP_GENERATORS: Range<usize> = 1..500;

    // need to generate 512-bit integers to ensure all operations
    // "wrap around" the group order during testing
    fn integer_512_bits() -> impl Strategy<Value = Integer> {
        any_with::<Bytes>(JUBJUB_MODULUS_BYTES.into())
            .prop_map(|bytes| Integer::from_digits(&bytes.as_ref(), BYTE_ORDER))
    }

    impl Arbitrary for GroupElement {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            integer_512_bits()
                .prop_map(move |value| Group::new_element(&value))
                .boxed()
        }
    }

    #[test]
    fn group_is_of_prime_order() {
        assert_ne!(Group::order().is_probably_prime(15), IsPrime::No)
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
        fn test_associative(x: GroupElement, y: GroupElement, z: GroupElement) {
            assert_eq!((x.clone() * y.clone()) * z.clone(), x * (y * z));
        }

        #[test]
        fn test_add_identity(element: GroupElement) {
            assert_eq!(element.clone() * Group::identity(), Group::identity() * element.clone());
            assert_eq!(element.clone() * Group::identity(), element);
            assert_eq!(element, Group::identity() * element.clone());

        }

        #[test]
        fn test_pow_prod(element: GroupElement, a in integer_512_bits(), b in integer_512_bits()) {
            let prod = a.clone() * b.clone();
            let expected = prod % Group::order();
            assert_eq!(element.pow(&a).pow(&b), element.pow(&expected))
        }

        #[test]
        fn test_pow_negative(element: GroupElement, a in integer_512_bits()) {
            let negative = -(a.clone() % Group::order());
            let expected = Group::order() - (a % Group::order());
            assert_eq!(element.pow(&negative), element.pow(&expected))
        }

        #[test]
        fn test_sums_in_exponent(element: GroupElement, a in integer_512_bits(), b in integer_512_bits()) {
            let expected = a.clone() + b.clone() % Group::order();
            assert_eq!(element.pow(&a) * element.pow(&b), element.pow(&expected))
        }

        #[test]
        fn test_generators_deterministic(
            num in NUM_GROUP_GENERATORS,
            seed: AESSeed) {
            assert_eq!(Group::generators(num, &seed), Group::generators(num, &seed));
        }

        #[test]
        fn test_generators_different_seeds_different_generators(
            num in NUM_GROUP_GENERATORS,
            seed1: AESSeed,
            seed2: AESSeed
        ) {
            prop_assume!(seed1 != seed2, "Different generators only for different seeds");
            assert_ne!(Group::generators(num, &seed1), Group::generators(num, &seed2));
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
