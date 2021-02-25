//! Spectrum implementation.
use crate::field::FieldTrait;
use crate::group::Sampleable;
use std::fmt::Debug;
use std::iter::{once, repeat_with};

/// message contains a vector of bytes representing data in spectrum
/// and is used for easily performing binary operations over bytes
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretShare<F> {
    value: F,
    is_first: bool,
}

pub trait Shareable {
    type Shares;

    fn share(self, n: usize) -> Vec<Self::Shares>;
    fn recover(shares: Vec<Self::Shares>) -> Self;
}

impl<F> SecretShare<F>
where
    F: Clone,
{
    /// generates new secret share in a field element
    pub fn new(value: F, is_first: bool) -> Self {
        Self { value, is_first }
    }

    pub fn value(&self) -> F {
        self.value.clone()
    }

    pub fn is_first(&self) -> bool {
        self.is_first
    }
}

impl<F> From<F> for SecretShare<F>
where
    F: FieldTrait,
{
    fn from(value: F) -> Self {
        Self {
            value,
            is_first: false,
        }
    }
}

impl<F> FieldTrait for SecretShare<F>
where
    F: FieldTrait,
{
    fn add(&self, rhs: &Self) -> Self {
        // is_first makes this not commutative!
        SecretShare {
            value: self.value.add(&rhs.value),
            is_first: self.is_first,
        }
    }

    fn neg(&self) -> Self {
        SecretShare {
            value: self.value.neg(),
            is_first: self.is_first,
        }
    }

    fn zero() -> Self {
        SecretShare {
            value: F::zero(),
            is_first: false,
        }
    }

    fn mul(&self, rhs: &Self) -> Self {
        SecretShare {
            value: self.value.mul(&rhs.value),
            is_first: self.is_first,
        }
    }

    fn mul_invert(&self) -> Self {
        SecretShare {
            value: self.value.mul_invert(),
            is_first: self.is_first,
        }
    }

    fn one() -> Self {
        SecretShare {
            value: F::one(),
            is_first: false,
        }
    }
}

impl<F> Shareable for F
where
    F: FieldTrait + Sampleable + Clone,
{
    type Shares = SecretShare<F>;

    /// shares the value such that summing all the shares recovers the value
    fn share(self, n: usize) -> Vec<Self::Shares> {
        assert!(n >= 2, "cannot split secret into fewer than two shares!");

        let values: Vec<_> = repeat_with(|| F::rand_element()).take(n - 1).collect();
        let sum = values.iter().fold(self, |a, b| a.add(b));
        let mut is_first = true;
        once(sum)
            .chain(values)
            .map(|value| {
                let share = SecretShare::new(value, is_first);
                is_first = false;
                share
            })
            .collect()
    }

    /// recovers the shares by subtracting all shares from the first share
    fn recover(shares: Vec<Self::Shares>) -> F {
        assert!(
            shares.len() >= 2,
            "need at least two shares to recover a secret!"
        );

        // recover the secret by subtracting the random shares (mask)
        shares
            .into_iter()
            .fold_first(|a, b| a.add(&b.neg()))
            .unwrap()
            .value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::IntegerMod128BitPrime as IntModP;
    use proptest::prelude::*;

    const MAX_SPLIT: usize = 100;

    proptest! {
        #[test]
        fn test_share_recover_identity(
            value: IntModP,
            num_shares in 2..MAX_SPLIT
        ) {
            prop_assert_eq!(
                IntModP::recover(value.clone().share(num_shares)),
                value
            )
        }

        #[test]
        fn test_share_randomized(
            value: IntModP,
            num_shares in 10..MAX_SPLIT  // Need more than 2 shares to avoid them being equal by chance
        ) {
            prop_assert_ne!(
                value.clone().share(num_shares),
                value.share(num_shares)
            );
        }

        #[test]
        fn test_homomorphic_constant_add(
            value: IntModP,
            constant: IntModP,
            num_shares in 2..MAX_SPLIT
        ) {
            let mut shares = value.clone().share(num_shares);
            // TODO: this doesn't work except for the first share!
            shares[0] = shares[0].add(&SecretShare::from(constant.clone()));
            prop_assert_eq!(
                IntModP::recover(shares),
                value.add(&constant)
            );
        }

        #[test]
        fn test_homomorphic_share_add(
            value1: IntModP,
            value2: IntModP,
            num_shares in 2..MAX_SPLIT
        ) {
            let shares = value1.clone().share(num_shares)
                .into_iter()
                .zip(value2.clone().share(num_shares).into_iter())
                .map(|(x, y)| x.add(&y))
                .collect();
            prop_assert_eq!(IntModP::recover(shares), value1.add(&value2));
        }

        #[test]
        fn test_homomorphic_constant_mul(
            value: IntModP,
            constant: IntModP,
            num_shares in 2..MAX_SPLIT
        ) {
            let shares = value.clone().share(num_shares)
                .into_iter()
                .map(|x| x.mul(&SecretShare::from(constant.clone())))
                .collect();
            prop_assert_eq!(
                IntModP::recover(shares),
                value.mul(&constant)
            );
        }

        #[test]
        #[should_panic]
        fn test_one_share_invalid(value: IntModP) {
            value.share(1);
        }
    }
}
