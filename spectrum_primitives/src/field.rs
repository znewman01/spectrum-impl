//! Spectrum implementation.
pub use crate::algebra::Field;
use crate::group::GroupElement;
use crate::util::Sampleable;

use rand::{thread_rng, Rng};
use rug::Integer;
// use rug::{integer::Order, Integer};

use std::convert::{TryFrom, TryInto};

#[cfg(feature = "proto")]
use crate::proto;

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

/// rug::Integer mod 2^128 - 159
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct IntegerMod128BitPrime {
    inner: Integer,
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
