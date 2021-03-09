//! Linear secret sharing.
use std::iter::{once, repeat_with};
use std::{fmt::Debug, ops::Add};

use itertools::Itertools;

use crate::algebra::{Field, Group};
use crate::util::Sampleable;

pub trait Shareable {
    type Share;

    fn share(self, n: usize) -> Vec<Self::Share>;
    fn recover(shares: Vec<Self::Share>) -> Self;
}

pub trait LinearlyShareable<T: Field>: Shareable<Share = T> {}

impl<G> Shareable for G
where
    G: Group + Sampleable + Clone,
{
    type Share = G;

    /// shares the value such that summing all the shares recovers the value
    fn share(self, n: usize) -> Vec<Self::Share> {
        assert!(n >= 2, "cannot split secret into fewer than two shares!");
        let values: Vec<_> = repeat_with(G::sample).take(n - 1).collect();
        let sum = values.iter().cloned().fold(G::zero(), Add::add);
        once(self - sum).chain(values).collect()
    }

    /// recovers the shares by subtracting all shares from the first share
    fn recover(shares: Vec<Self::Share>) -> Self {
        assert!(
            shares.len() >= 2,
            "need at least two shares to recover a secret!"
        );
        shares.into_iter().fold(G::zero(), Add::add)
    }
}

impl<F> LinearlyShareable<F> for F where F: Shareable<Share = F> + Field + Sampleable + Clone {}

fn transpose<T: Debug>(vec: Vec<Vec<T>>) -> Vec<Vec<T>> {
    if vec.is_empty() {
        return vec;
    }
    let inner_len = vec
        .iter()
        .map(Vec::len)
        .dedup()
        .exactly_one()
        .expect("All inner vecs should have the same length.");
    let mut transposed: Vec<Vec<T>> = repeat_with(|| Vec::with_capacity(vec.len()))
        .take(std::cmp::max(inner_len, 1))
        .collect();
    for inner in vec.into_iter() {
        for (idx, value) in inner.into_iter().enumerate() {
            transposed[idx].push(value);
        }
    }
    transposed
}

#[cfg(test)]
mod transpose_tests {
    use super::*;
    use proptest::prelude::*;

    /// Strategy for generating rectangular Vec<Vec<T>> (i.e., inner vecs have the same len()).
    fn rectangular_nonempty<T: Arbitrary>() -> impl Strategy<Value = Vec<Vec<T>>> {
        use prop::collection::vec;
        (1..100usize).prop_flat_map(|n| vec(vec(any::<T>(), n), 1..100usize))
    }

    #[test]
    fn test_transpose_empty() {
        let zero_by_n: Vec<Vec<u8>> = vec![];
        assert_eq!(transpose(zero_by_n), vec![]: Vec::<Vec<u8>>);

        let one_by_zero: Vec<Vec<u8>> = vec![vec![]];
        assert_eq!(transpose(one_by_zero), vec![vec![]]: Vec<Vec<u8>>);

        let n_by_zero: Vec<Vec<u8>> = vec![vec![], vec![]];
        assert_eq!(transpose(n_by_zero), vec![vec![]]: Vec<Vec<u8>>);
    }

    proptest! {
        #[test]
        fn test_transpose_self_inverse(vec in rectangular_nonempty::<u8>()) {
           prop_assert_eq!(vec.clone(), transpose(transpose(vec)));
        }

        #[test]
        fn test_transpose_dims(vec in rectangular_nonempty::<u8>()) {
            vec.iter().map(Vec::len).dedup().exactly_one().expect("expected rectangular vec<vec<_>>");

            let outer = vec.len();
            assert_ne!(outer, 0);
            let inner = vec.iter().map(Vec::len).next().unwrap();
            assert_ne!(inner, 0);

            let transposed = transpose(vec);

            prop_assert_eq!(transposed.len(), inner);
            prop_assert_eq!(transposed.iter().map(Vec::len).next().unwrap_or(0), outer);
        }
    }
}

impl<S> Shareable for Vec<S>
where
    S: Shareable,
    <S as Shareable>::Share: Debug,
{
    type Share = Vec<S::Share>;

    fn share(self, n: usize) -> Vec<Self::Share> {
        assert!(n >= 2, "cannot split secret into fewer than two shares!");
        transpose(self.into_iter().map(|v| v.share(n)).collect())
    }

    fn recover(shares: Vec<Self::Share>) -> Self {
        transpose(shares).into_iter().map(S::recover).collect()
    }
}

impl Shareable for bool {
    type Share = bool;

    fn share(self, n: usize) -> Vec<bool> {
        assert!(n >= 2, "cannot split secret into fewer than two shares!");
        use rand::prelude::*;
        let mut shares: Vec<bool> = repeat_with(|| thread_rng().gen()).take(n - 1).collect();
        let parity = shares.iter().fold(false, std::ops::BitXor::bitxor);
        shares.push(parity ^ self);
        shares
    }

    fn recover(shares: Vec<bool>) -> bool {
        shares.iter().fold(false, std::ops::BitXor::bitxor)
    }
}

#[cfg(test)]
macro_rules! check_shareable_norandom {
    ($type:ty) => {
        mod basic {
            #![allow(unused_imports)]
            use super::*;
            use crate::sharing::Shareable;
            use proptest::prelude::*;
            const MAX_SHARES: usize = 100;
            proptest! {
                #[test]
                fn test_share_recover_identity(value: $type, num_shares in 2..MAX_SHARES) {
                    let shares = value.clone().share(num_shares);
                    prop_assert_eq!(<$type as Shareable>::recover(shares), value);
                }

                #[test]
                #[should_panic]
                fn test_one_share_invalid(value: $type) {
                    value.share(1);
                }
            }
        }
    };
}

#[cfg(test)]
macro_rules! check_shareable {
    ($type:ty) => {
        mod sharing {
            #![allow(unused_imports)]
            use super::*;
            use crate::sharing::Shareable;
            use proptest::prelude::*;
            const MAX_SHARES: usize = 100;

            check_shareable_norandom!($type);

            proptest! {
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
            }
        }
    };
}

#[cfg(test)]
macro_rules! check_linearly_shareable {
    ($type:ty,$mod_name:ident) => {
        mod $mod_name {
            #![allow(unused_imports)]
            use super::*;
            use crate::sharing::{LinearlyShareable, Shareable};
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

#[cfg(test)]
mod tests {
    mod bool {
        check_shareable!(bool);
    }

    mod vec {
        check_shareable_norandom!(Vec<bool>);
    }
}
