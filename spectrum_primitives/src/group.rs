//! Spectrum implementation.
use crate::algebra::Group;
use crate::bytes::Bytes;
use crate::prg::{aes::AESSeed, aes::AESPRG, PRG};

use rand::prelude::*;
use rug::{integer::Order, Integer};
use serde::{de, ser::Serializer, Deserialize, Serialize};

use std::convert::{From, TryFrom, TryInto};
use std::fmt::Debug;
use std::hash::Hash;
use std::hash::Hasher;
use std::ops;

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

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

    check_group_laws! { u8 }
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
    pub(in crate) inner: Jubjub,
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::bytes::Bytes;

    use rug::integer::IsPrime;

    use std::ops::Range;

    proptest! {
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
