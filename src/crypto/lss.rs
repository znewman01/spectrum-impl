//! Spectrum implementation.
use crate::crypto::field::{Field, FieldElement};
use rug::{rand::RandState, Integer};
use std::fmt::Debug;
use std::ops;
use std::rc::Rc;

/// message contains a vector of bytes representing data in spectrum
/// and is used for easily performing binary operations over bytes
#[derive(Clone, Debug)]
pub struct SecretShare {
    value: FieldElement,
}

/// linear secret sharing functionality
pub struct LSS {}

impl SecretShare {
    /// generates new secret share in a field element
    fn new(value: FieldElement) -> SecretShare {
        SecretShare { value }
    }

    pub fn scalar_add(&mut self, scalar: FieldElement) {
        self.value = self.value.clone() + scalar
    }

    pub fn scalar_mul(&mut self, scalar: FieldElement) {
        self.value = self.value.clone() * scalar
    }
}

impl LSS {
    pub fn share(value: FieldElement, n: usize, rng: &mut RandState) -> Vec<SecretShare> {
        if n < 2 {
            panic!("cannot split secret into fewer than two shares!");
        }

        let mut shares = Vec::<SecretShare>::new();
        let mut rand_sum = FieldElement::new(Integer::from(0), value.clone().field());

        let field: Rc<Field> = value.clone().field();

        // first share will be value + SUM r_i
        shares.push(SecretShare::new(value));

        for _ in 0..n - 1 {
            let rand_i = FieldElement::rand_element(rng, field.clone());
            shares.push(SecretShare::new(rand_i.clone()));
            rand_sum += rand_i;
        }

        shares[0].value += rand_sum;

        shares
    }

    pub fn recover(shares: Vec<SecretShare>) -> FieldElement {
        if shares.len() < 2 {
            panic!("need at least two shares to recover a secret!");
        }

        // recover the secret by subtracting the random shares (mask)
        let mut secret = shares[0].value.clone();
        for share in shares.iter().skip(1) {
            secret -= share.value.clone();
        }

        secret
    }
}

impl PartialEq for SecretShare {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

/// override + operation: want operation over the field value and sequence of operations to be updated
impl ops::Add<SecretShare> for SecretShare {
    type Output = SecretShare;

    fn add(self, other: SecretShare) -> SecretShare {
        SecretShare::new(self.value + other.value)
    }
}

/// override - operation: want operation over the field value and sequence of operations to be updated
impl ops::Sub<SecretShare> for SecretShare {
    type Output = SecretShare;

    fn sub(self, other: SecretShare) -> SecretShare {
        SecretShare::new(self.value - other.value)
    }
}

/// override * operation: want multiplication by constant field element
impl ops::Mul<FieldElement> for SecretShare {
    type Output = SecretShare;

    fn mul(self, constant: FieldElement) -> SecretShare {
        SecretShare::new(self.value * constant)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::field::Field;
    use std::rc::Rc;

    // TODO(sss): Add tests for add/sub/mul, etc.

    #[test]
    fn test_share_recover() {
        let mut rng = RandState::new();
        let field = Field::new(Integer::from(101)); // 101 is prime
        let value = FieldElement::new(Integer::from(100), Rc::<Field>::new(field));

        assert_eq!(value.clone(), LSS::recover(LSS::share(value, 10, &mut rng)));
    }

    #[test]
    fn test_share_splitting() {
        let mut rng = RandState::new();
        let field = Field::new(Integer::from(101)); // 101 is prime
        let value = FieldElement::new(Integer::from(100), Rc::<Field>::new(field));

        // Share generates different shares each time
        assert_ne!(
            LSS::share(value.clone(), 10, &mut rng),
            LSS::share(value, 10, &mut rng)
        );
    }

    #[test]
    fn test_share_scalar_add() {
        let mut rng = RandState::new();
        let field = Rc::<Field>::new(Field::new(Integer::from(101))); // 101 is prime
        let value = FieldElement::new(Integer::from(100), field.clone());
        let mut shares = LSS::share(value.clone(), 10, &mut rng);
        let scalar = FieldElement::new(Integer::from(5), field);
        shares[0].scalar_add(scalar.clone());
        assert_eq!(value + scalar, LSS::recover(shares));
    }

    #[test]
    fn test_share_scalar_mul() {
        let mut rng = RandState::new();
        let field = Rc::<Field>::new(Field::new(Integer::from(101))); // 101 is prime
        let value = FieldElement::new(Integer::from(100), field.clone());
        let mut shares = LSS::share(value.clone(), 10, &mut rng);
        let scalar = FieldElement::new(Integer::from(5), field);
        for share in shares.iter_mut() {
            share.scalar_mul(scalar.clone());
        }
        assert_eq!(value * scalar, LSS::recover(shares));
    }

    #[test]
    fn test_share_different_n() {
        let mut rng = RandState::new();
        let field = Field::new(Integer::from(101)); // 101 is prime
        let value = FieldElement::new(Integer::from(100), Rc::<Field>::new(field));

        let rec2 = LSS::recover(LSS::share(value.clone(), 2, &mut rng));
        let rec3 = LSS::recover(LSS::share(value.clone(), 3, &mut rng));
        let rec4 = LSS::recover(LSS::share(value.clone(), 4, &mut rng));
        let rec5 = LSS::recover(LSS::share(value, 5, &mut rng));

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
        LSS::share(value, 1, &mut rng);
    }
}
