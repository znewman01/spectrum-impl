use std::ops;

use rug::Integer;

/// A *commutative* group
///
/// Group operation must be [`Add`].
///
/// [`Add`]: std::ops::Add;
pub trait Group:
    Eq
    + ops::Sub<Output = Self>
    + ops::Add<Output = Self>
    + ops::Neg<Output = Self>
    + ops::Mul<Integer, Output = Self>
    + Sized
{
    fn order() -> Integer;
    fn order_size_in_bytes() -> usize {
        Self::order().significant_digits::<u8>()
    }
    fn zero() -> Self;
}

#[cfg(any(test, feature = "testing"))]
macro_rules! check_group_laws {
    ($type:ty,$mod_name:ident) => {
        // wish I could use concat_idents!(group_laws, $type) here
        mod $mod_name {
            #![allow(unused_imports)]
            use super::*;
            use proptest::prelude::*;
            use rug::Integer;
            proptest! {
              #[test]
              fn test_associative(a: $type, b: $type, c: $type) {
                  let a2 = a.clone();
                  let b2 = b.clone();
                  let c2 = c.clone();
                  prop_assert_eq!((a + b) + c, a2 + (b2 + c2));
              }

              #[test]
              fn test_commutative(a: $type, b: $type) {
                  let a2 = a.clone();
                  let b2 = b.clone();
                  prop_assert_eq!(a + b, b2 + a2);
              }

              #[test]
              fn test_zero(a: $type) {
                  let a2 = a.clone();
                  prop_assert_eq!(a + <$type as Group>::zero(), a2);
              }

              #[test]
              fn test_inverse(a: $type) {
                  let a2 = a.clone();
                  prop_assert_eq!(a + (-a2), <$type as Group>::zero());
              }

              // exp *should* be an integer but the way we're checking is a loop so don't want it too big
              #[test]
              fn test_exponent_definition(base: $type, exp: u16) {
                  let actual = base.clone() * Integer::from(exp);
                  let mut expected = <$type as Group>::zero();
                  for _ in 0..exp {
                      expected = expected + base.clone();
                  }
                  prop_assert_eq!(actual, expected);
              }
            }
        }
    };
    ($type:ty) => {
        check_group_laws!($type, group_laws);
    };
}

// If I were really clever, I'd give a general implementation with two
// commutative groups, over Self and NonZero<Self> (plus distributivity).
pub trait Field: Eq + PartialEq + ops::Mul<Output = Self> + Sized + Group {
    /// Take the multiplicative inverse.
    ///
    /// Panics if you pass zero().
    fn mul_invert(&self) -> Self;

    /// Additive identity
    fn zero() -> Self {
        <Self as Group>::zero()
    }

    /// The multiplicative identity.
    fn one() -> Self;
}

#[cfg(any(test, feature = "testing"))]
macro_rules! check_field_laws {
    ($type:ty,$mod_name:ident) => {
        mod $mod_name {
            #![allow(unused_imports)]
            use super::*;
            use proptest::prelude::*;
            proptest! {
                #[test]
                fn test_mul_associative(a: $type, b: $type, c: $type) {
                    let a2 = a.clone();
                    let b2 = b.clone();
                    let c2 = c.clone();
                    prop_assert_eq!((a * b) * c, a2 * (b2 * c2));
                }

                #[test]
                fn test_mul_commutative(a: $type, b: $type) {
                    let a2 = a.clone();
                    let b2 = b.clone();
                    prop_assert_eq!(a * b, b2 * a2);
                }

                #[test]
                fn test_mul_identity(a: $type) {
                    let a2 = a.clone();
                    prop_assert_eq!(a * (<$type as Field>::one()), a2);
                }

                #[test]
                fn test_mul_inverse(a: $type) {
                    prop_assume!(a != <$type as Field>::zero());
                    let a2 = a.clone();
                    prop_assert_eq!(a * a2.mul_invert(), <$type as Field>::one());
                }

                #[test]
                fn test_distributive(a: $type, b: $type, c: $type) {
                    let a2 = a.clone();
                    let a3 = a.clone();
                    let b2 = b.clone();
                    let c2 = c.clone();
                    prop_assert_eq!(a * (b + c), a2 * b2 + a3 * c2);
                }
            }
        }
    };
    ($type:ty) => {
        check_field_laws!($type, field_laws);
    };
}
