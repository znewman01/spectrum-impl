//! Linear secret sharing.
use crate::algebra::Field;
use crate::util::Sampleable;
use std::iter::{once, repeat_with, Sum};

pub trait Shareable {
    type Share;

    fn share(self, n: usize) -> Vec<Self::Share>;
    fn recover(shares: Vec<Self::Share>) -> Self;
}

pub trait LinearlyShareable<T>: Shareable<Share = T> {}

impl<F> Shareable for F
where
    F: Field + Sampleable + Clone + Sum,
{
    type Share = F;

    /// shares the value such that summing all the shares recovers the value
    fn share(self, n: usize) -> Vec<Self::Share> {
        assert!(n >= 2, "cannot split secret into fewer than two shares!");
        let values: Vec<_> = repeat_with(F::sample).take(n - 1).collect();
        let sum = values.iter().cloned().sum();
        once(self - sum).chain(values).collect()
    }

    /// recovers the shares by subtracting all shares from the first share
    fn recover(shares: Vec<Self::Share>) -> F {
        assert!(
            shares.len() >= 2,
            "need at least two shares to recover a secret!"
        );
        shares.into_iter().sum()
    }
}

impl<S, T> LinearlyShareable<T> for S
where
    T: Field,
    S: Shareable<Share = T>,
{
}

#[cfg(any(test, feature = "testing"))]
macro_rules! check_shareable {
    ($type:ty,$mod_name:ident) => {
        mod $mod_name {
            #![allow(unused_imports)]
            use super::*;
            use proptest::prelude::*;
            const MAX_SHARES: usize = 100;
            proptest! {
                #[test]
                fn test_share_recover_identity(value: $type, num_shares in 2..MAX_SHARES) {
                    let shares = value.clone().share(num_shares);
                    prop_assert_eq!(<$type as Shareable>::recover(shares), value);
                }

                #[test]
                fn test_share_randomized(
                    value: $type,
                    num_shares in 10..MAX_SHARES  // Need >>2 shares to avoid them being equal by chance
                ) {
                    prop_assert_ne!(
                        value.clone().share(num_shares),
                        value.share(num_shares)
                    );
                }

                #[test]
                #[should_panic]
                fn test_one_share_invalid(value: $type) {
                    value.share(1);
                }
            }
        }
    };
    ($type:ty) => {
        check_shareable!($type, shareable);
    };
}

#[cfg(any(test, feature = "testing"))]
macro_rules! check_linearly_shareable {
    ($type:ty,$mod_name:ident) => {
        mod $mod_name {
            #![allow(unused_imports)]
            use super::*;
            use proptest::prelude::*;
            const MAX_SHARES: usize = 100;

            fn is_bounded<S: Field, T: LinearlyShareable<S>>() {}
            #[test]
            fn test_bounds() {
                is_bounded::<<$type as Shareable>::Share, $type>()
            }

            proptest! {
                /// Adding a constant to *any* share should give the
                /// original shared value plus constant on recovery.
                #[test]
                fn test_constant_add(
                    value: $type,
                    constant: $type,
                    num_shares in 2..MAX_SHARES,
                    index: prop::sample::Index,
                ) {
                    let idx = index.index(num_shares);
                    let mut shares = value.clone().share(num_shares);
                    shares[idx] += constant.clone();
                    prop_assert_eq!(
                        <$type as Shareable>::recover(shares),
                        value + constant
                    );
                }

                /// Adding shares of two values together element wise should
                /// give the sum of the two values on recovery.
                #[test]
                fn test_share_add(
                    value1: $type,
                    value2: $type,
                    num_shares in 2..MAX_SHARES
                ) {
                    let shares = value1.clone().share(num_shares)
                        .into_iter()
                        .zip(value2.clone().share(num_shares).into_iter())
                        .map(|(x, y)| x + y)
                        .collect();
                    prop_assert_eq!(<$type as Shareable>::recover(shares), value1 + value2);
                }

                /// Multiplying all shares by a constant should give the
                /// original shared value times on recovery.
                #[test]
                fn test_constant_mul(
                    value: $type,
                    constant: $type,
                    num_shares in 2..MAX_SHARES
                ) {
                    let shares = value.clone().share(num_shares)
                        .into_iter()
                        .map(|x| x * constant.clone())
                        .collect();
                    prop_assert_eq!(
                        <$type as Shareable>::recover(shares),
                        value * constant
                    );
                }

            }
        }
    };
    ($type:ty) => {
        check_linearly_shareable!($type, linearly_shareable);
    };
}
