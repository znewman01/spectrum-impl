//! Spectrum implementation.
use crate::crypto::field::FieldElement;
use rug::{rand::RandState, Integer};
use std::fmt::Debug;
use std::ops;

/// message contains a vector of bytes representing data in spectrum
/// and is used for easily performing binary operations over bytes
#[derive(Clone, Debug)]
pub struct SecretShare {
    value: FieldElement,
    is_first: bool,
}

impl SecretShare {
    /// generates a new field element; value mod field.order
    fn new(value: FieldElement, is_first: bool) -> SecretShare {
        SecretShare { value, is_first }
    }
}

impl PartialEq for SecretShare {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

pub fn share(value: FieldElement, n: usize, rng: &mut RandState) -> Vec<SecretShare> {
    if n < 2 {
        panic!("cannot split secret into fewer than two shares!");
    }

    let mut shares = Vec::<SecretShare>::new();
    let mut rand_sum = FieldElement::new(Integer::from(0), value.clone().field());

    // TODO: simplify this; something is probably being done incorrectly.
    let field = value.clone().field().as_ref().clone();

    // first share will be value + SUM r_i
    shares.push(SecretShare::new(value, true));

    for _ in 0..n - 1 {
        let rand_i = field.clone().rand_element(rng);
        shares.push(SecretShare::new(rand_i.clone(), false));
        rand_sum += rand_i;
    }

    shares[0].value += rand_sum;

    shares
}

pub fn recover(shares: Vec<SecretShare>) -> FieldElement {
    if shares.len() < 2 {
        panic!("need at least two shares to recover a secret!");
    }

    // recover the secret by subtracting random shares
    let mut secret = shares[0].value.clone();
    for share in shares.iter().skip(1) {
        secret -= share.value.clone();
    }

    secret
}

/// override + operation: want operation over the field value and sequence of operations to be updated
impl ops::Add<SecretShare> for SecretShare {
    type Output = SecretShare;

    fn add(self, other: SecretShare) -> SecretShare {
        SecretShare::new(self.value + other.value, self.is_first)
    }
}

/// override - operation: want operation over the field value and sequence of operations to be updated
impl ops::Sub<SecretShare> for SecretShare {
    type Output = SecretShare;

    fn sub(self, other: SecretShare) -> SecretShare {
        SecretShare::new(self.value - other.value, self.is_first)
    }
}

/// override * operation: want multiplication by constant field element
impl ops::Mul<FieldElement> for SecretShare {
    type Output = SecretShare;

    fn mul(self, constant: FieldElement) -> SecretShare {
        SecretShare::new(self.value * constant, self.is_first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::field::Field;
    use std::rc::Rc;

    #[test]
    fn test_share_recover() {
        let mut rng = RandState::new();
        let field = Field::new(Integer::from(101)); // 101 is prime
        let value = FieldElement::new(Integer::from(100), Rc::<Field>::new(field));

        assert_eq!(value.clone(), recover(share(value, 10, &mut rng)));
    }

    #[test]
    fn test_share_splitting() {
        let mut rng = RandState::new();
        let field = Field::new(Integer::from(101)); // 101 is prime
        let value = FieldElement::new(Integer::from(100), Rc::<Field>::new(field));

        // Share generates different shares each time
        assert_ne!(
            share(value.clone(), 10, &mut rng),
            share(value, 10, &mut rng)
        );
    }

    #[test]
    fn test_share_different_n() {
        let mut rng = RandState::new();
        let field = Field::new(Integer::from(101)); // 101 is prime
        let value = FieldElement::new(Integer::from(100), Rc::<Field>::new(field));

        let rec2 = recover(share(value.clone(), 2, &mut rng));
        let rec3 = recover(share(value.clone(), 3, &mut rng));
        let rec4 = recover(share(value.clone(), 4, &mut rng));
        let rec5 = recover(share(value, 5, &mut rng));

        // sharing with different n works
        assert_eq!(rec2, rec3);
        assert_eq!(rec3, rec4);
        assert_eq!(rec4, rec5);
    }

    #[test]
    #[should_panic]
    fn test_share_invalid() {
        let mut rng = RandState::new();
        let field = Field::new(Integer::from(101)); // 101 is prime
        let value = FieldElement::new(Integer::from(100), Rc::<Field>::new(field));
        share(value, 1, &mut rng);
    }
}
