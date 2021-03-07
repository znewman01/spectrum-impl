use std::ops;

use rug::Integer;

/// A monoid (over the `+` operator).
///
/// Must be associative and have an identity.
pub trait Monoid: Eq + ops::Add<Output = Self> + Sized {
    fn zero() -> Self;
}

#[cfg(any(test, feature = "testing"))]
macro_rules! check_monoid_laws {
    ($type:ty,$mod_name:ident) => {
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
              fn test_zero(a: $type) {
                  let a2 = a.clone();
                  let a3 = a.clone();
                  let a4 = a.clone();
                  // no commutativity so have to check both
                  prop_assert_eq!(a, a2 + <$type as Monoid>::zero());
                  prop_assert_eq!(a4, <$type as Monoid>::zero() + a3);
              }
            }
        }
    };
    ($type:ty) => {
        check_monoid_laws!($type, monoid_laws);
    };
}

/// A monoid with custom exponentiation for a particular exponent type.
pub trait SpecialExponentMonoid: Monoid {
    type Exponent: Monoid;

    /// Raise `self` to the `exp`th power.
    fn pow(&self, exp: Self::Exponent) -> Self;
}

#[cfg(any(test, feature = "testing"))]
macro_rules! check_monoid_custom_exponent {
    ($type:ty,$mod_name:ident) => {
        mod mod_name {
            #![allow(unused_imports)]
            check_monoid_laws!($type);
            use super::*;
            use proptest::prelude::*;
            use rug::Integer;
            proptest! {
                /// Check x^(a+b) == x^a * x^b.
                ///
                /// We're using `+` and `.pow` for the monoid operation and
                /// exponentiation, respectively.
                #[test]
                fn test_exponent_sum(
                    base: $type,
                    exp1: <$type as SpecialExponentMonoid>::Exponent,
                    exp2: <$type as SpecialExponentMonoid>::Exponent
                ) {
                    prop_assert_eq!(
                      base.clone().pow(exp1.clone()) + base.clone().pow(exp2.clone()),
                      base.pow(exp1 + exp2)
                    );
                }

                /// Check x^(a*b) == (x^a)^b.
                #[test]
                fn test_exponent_product(
                    base: $type,
                    exp1: <$type as SpecialExponentMonoid>::Exponent,
                    exp2: <$type as SpecialExponentMonoid>::Exponent
                ) {
                    prop_assert_eq!(
                      base.clone().pow(exp1.clone()).pow(exp2.clone()),
                      base.pow(exp1 * exp2)
                    );
                }

                /// Check (x*y)^a == x^a * y^a
                #[test]
                fn test_exponent_distributive(
                    base1: $type,
                    base2: $type,
                    exp: <$type as SpecialExponentMonoid>::Exponent,
                ) {
                    prop_assert_eq!(
                      (base1.clone() + base2.clone()).pow(exp.clone()),
                      base1.pow(exp.clone()) + base2.pow(exp)
                    );
                }
            }
        }
    };
    ($type:ty) => {
        check_monoid_custom_exponent!($type, monoid_custom_exponent);
    };
}

/// A *commutative* group
///
/// Group operation must be [`Add`].
///
/// [`Add`]: std::ops::Add;
pub trait Group: Monoid + ops::Sub<Output = Self> + ops::Neg<Output = Self> + Sized {
    fn order() -> Integer;
    fn order_size_in_bytes() -> usize {
        Self::order().significant_digits::<u8>()
    }
}

#[cfg(any(test, feature = "testing"))]
macro_rules! check_group_laws {
    ($type:ty,$mod_name:ident) => {
        // wish I could use concat_idents!(group_laws, $type) here
        mod $mod_name {
            #![allow(unused_imports)]
            check_monoid_laws!($type);
            use super::*;
            use proptest::prelude::*;
            use rug::Integer;
            proptest! {
              #[test]
              #[test]
              fn test_commutative(a: $type, b: $type) {
                  let a2 = a.clone();
                  let b2 = b.clone();
                  prop_assert_eq!(a + b, b2 + a2);
              }

              #[test]
              fn test_inverse(a: $type) {
                  let a2 = a.clone();
                  prop_assert_eq!(a + (-a2), <$type as Monoid>::zero());
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
                    prop_assume!(a != <$type as Monoid>::zero());
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
