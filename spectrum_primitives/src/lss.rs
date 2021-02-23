//! Spectrum implementation.
use crate::field::FieldElement;
use rug::rand::RandState;
use std::fmt::Debug;
use std::iter::{once, repeat_with};
use std::ops;

/// message contains a vector of bytes representing data in spectrum
/// and is used for easily performing binary operations over bytes
#[derive(Clone, Debug)]
pub struct SecretShare {
    value: FieldElement,
    is_first: bool,
}

pub trait Shareable {
    type Shares;

    fn share(self, n: usize, rng: &mut RandState) -> Vec<Self::Shares>;
    fn recover(shares: Vec<Self::Shares>) -> Self;
}

impl SecretShare {
    /// generates new secret share in a field element
    pub fn new(value: FieldElement, is_first: bool) -> SecretShare {
        SecretShare { value, is_first }
    }

    pub fn value(&self) -> FieldElement {
        self.value.clone()
    }

    pub fn is_first(&self) -> bool {
        self.is_first
    }
}

impl Shareable for FieldElement {
    type Shares = SecretShare;

    /// shares the value such that summing all the shares recovers the value
    fn share(self, n: usize, rng: &mut RandState) -> Vec<SecretShare> {
        assert!(n >= 2, "cannot split secret into fewer than two shares!");

        let field = self.field();
        let values: Vec<_> = repeat_with(|| field.rand_element(rng))
            .take(n - 1)
            .collect();
        let sum = values.iter().fold(self, |a, b| a + b.clone());
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
    fn recover(shares: Vec<SecretShare>) -> FieldElement {
        assert!(
            shares.len() >= 2,
            "need at least two shares to recover a secret!"
        );

        // recover the secret by subtracting the random shares (mask)
        shares
            .iter()
            .skip(1)
            .fold(shares[0].value.clone(), |a, b| a - b.value.clone())
    }
}

impl PartialEq for SecretShare {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl ops::Add<SecretShare> for SecretShare {
    type Output = SecretShare;

    fn add(self, other: SecretShare) -> SecretShare {
        SecretShare::new(self.value + other.value, self.is_first)
    }
}

impl ops::AddAssign<SecretShare> for SecretShare {
    fn add_assign(&mut self, other: SecretShare) {
        self.value += other.value;
    }
}

impl ops::Sub<SecretShare> for SecretShare {
    type Output = SecretShare;

    fn sub(self, other: SecretShare) -> SecretShare {
        SecretShare::new(self.value - other.value, self.is_first)
    }
}

impl ops::SubAssign<FieldElement> for SecretShare {
    fn sub_assign(&mut self, other: FieldElement) {
        self.value -= other;
    }
}

impl ops::Add<FieldElement> for SecretShare {
    type Output = SecretShare;

    fn add(self, constant: FieldElement) -> SecretShare {
        SecretShare::new(self.value + constant, self.is_first)
    }
}

impl ops::AddAssign<FieldElement> for SecretShare {
    fn add_assign(&mut self, other: FieldElement) {
        self.value += other;
    }
}

impl ops::Mul<FieldElement> for SecretShare {
    type Output = SecretShare;

    fn mul(self, constant: FieldElement) -> SecretShare {
        SecretShare::new(self.value * constant, self.is_first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::testing::field_element_pairs;
    use proptest::prelude::*;

    const MAX_SPLIT: usize = 100;

    proptest! {
        #[test]
        fn test_share_recover_identity(
            value in any::<FieldElement>(),
            num_shares in 2..MAX_SPLIT
        ) {
            let mut rng = RandState::new();
            prop_assert_eq!(
                FieldElement::recover(value.clone().share(num_shares, &mut rng)),
                value
            )
        }

        #[test]
        fn test_share_randomized(
            value in any::<FieldElement>(),
            num_shares in 10..MAX_SPLIT  // Need more than 2 shares to avoid them being equal by chance
        ) {
            let mut rng = RandState::new();
            prop_assert_ne!(
                value.clone().share(num_shares, &mut rng),
                value.share(num_shares, &mut rng)
            );
        }

        #[test]
        fn test_homomorphic_constant_add(
            (value, constant) in field_element_pairs(),
            num_shares in 2..MAX_SPLIT
        ) {
            let mut rng = RandState::new();
            prop_assert_eq!(
                FieldElement::recover(value.clone().share(num_shares, &mut rng)) + constant.clone(),
                value + constant
            );
        }

        #[test]
        fn test_homomorphic_share_add(
            (value1, value2) in field_element_pairs(),
            num_shares in 2..MAX_SPLIT
        ) {
            let mut rng = RandState::new();
            let shares = value1.clone().share(num_shares, &mut rng)
                .into_iter()
                .zip(value2.clone().share(num_shares, &mut rng).into_iter())
                .map(|(x, y)| x + y)
                .collect();
            prop_assert_eq!(FieldElement::recover(shares), value1 + value2);
        }

        #[test]
        fn test_homomorphic_constant_mul(
            (value, constant) in field_element_pairs(),
            num_shares in 2..MAX_SPLIT
        ) {
            let mut rng = RandState::new();
            let shares = value.clone().share(num_shares, &mut rng)
                .into_iter()
                .map(|x| x * constant.clone())
                .collect();
            prop_assert_eq!(
                FieldElement::recover(shares),
                value * constant
            );
        }

        #[test]
        #[should_panic]
        fn test_one_share_invalid(value in any::<FieldElement>()) {
            let mut rng = RandState::new();
            value.share(1, &mut rng);
        }
    }
}
