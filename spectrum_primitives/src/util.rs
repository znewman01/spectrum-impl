// use crate::prg::aes::AESSeed;

pub trait Sampleable {
    type Seed;

    /// generates a new random group element
    fn sample() -> Self;

    fn sample_from_seed(seed: &Self::Seed) -> Self
    where
        Self: Sized,
    {
        Self::sample_many_from_seed(seed, 1)
            .pop()
            .expect("Should have (exactly) one seed")
    }

    fn sample_many_from_seed(seed: &Self::Seed, n: usize) -> Vec<Self>
    where
        Self: Sized;
}

#[cfg(test)]
macro_rules! check_sampleable {
    ($type:ty) => {
        mod sampleable {
            #![allow(unused_imports)]
            use super::*;
            use proptest::prelude::*;
            use std::iter::repeat_with;
            #[test]
            fn test_not_deterministic() {
                use std::collections::HashSet;
                let elements: HashSet<_> = repeat_with(<$type>::sample).take(10).collect();
                assert!(
                    elements.len() > 1,
                    "Many random elements should not all be the same."
                );
            }

            proptest! {
                #[test]
                fn test_from_seed_deterministic(seed: <$type as Sampleable>::Seed) {
                    prop_assert_eq!(
                        <$type as Sampleable>::sample_from_seed(&seed),
                        <$type as Sampleable>::sample_from_seed(&seed)
                    );
                }

                #[test]
                fn test_many_from_seed_deterministic(seed: <$type as Sampleable>::Seed, n in 0..20usize) {
                    prop_assert_eq!(
                        <$type as Sampleable>::sample_many_from_seed(&seed, n),
                        <$type as Sampleable>::sample_many_from_seed(&seed, n)
                    );
                }

                #[test]
                fn test_many_from_seed_correct_count(seed: <$type as Sampleable>::Seed, n in 0..20usize) {
                    prop_assert_eq!(
                        <$type as Sampleable>::sample_many_from_seed(&seed, n).len(),
                        n
                    );
                }
            }
        }
    };
}

/// Test that `f(g(x)) == x` for all `x` of a particular type.
///
/// The type must implement [`Arbitrary`] and [`Clone`].
///
/// Last argument is an (optional) name for the submodule where this will go.
///
/// # Examples
///
/// ```
/// # #[macro_use] extern crate spectrum_primitives;
/// # fn main() {
/// fn plus_one(x: u8) -> u8 {
///   x + 1
/// }
/// check_roundtrip!(u8, plus_one, |x| x - 1, u8_plus_minus_one);
/// # }
/// ```
///
/// [`Arbitrary`]: proptest::arbitrary::Arbitrary
/// [`Clone`]: std::clone::Clone
#[cfg(any(test, feature = "testing"))]
#[macro_export]
macro_rules! check_roundtrip {
    ($type:ty,$to:expr,$from:expr,$name:ident) => {
        check_roundtrip!($type, any::<$type>(), $to, $from, $name);
    };
    ($type:ty,$strat:expr,$to:expr,$from:expr,$name:ident) => {
        mod $name {
            #![allow(unused_imports)]
            use super::*;
            use proptest::prelude::*;
            proptest! {
                #[test]
                fn test_roundtrip(x in $strat) {
                    prop_assert_eq!(($from)(($to)(x.clone())): $type, x: $type, "round-trip failed");
                }
            }
        }
    };
    ($type:ty,$to:expr,$from:expr) => {
        check_roundtrip!($type, $to, $from, roundtrip);
    };
}
