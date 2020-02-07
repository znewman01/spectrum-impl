//! Spectrum implementation.
use crate::crypto::field::{Field, FieldElement};
use rug::rand::RandState;
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
}

impl LSS {
    pub fn share(value: FieldElement, n: usize, rng: &mut RandState) -> Vec<SecretShare> {
        assert!(n >= 2, "cannot split secret into fewer than two shares!");

        let mut shares = Vec::<SecretShare>::new();
        let mut rand_sum = FieldElement::new(0.into(), value.clone().field());

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
        assert!(
            shares.len() >= 2,
            "need at least two shares to recover a secret!"
        );

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

impl ops::Add<FieldElement> for SecretShare {
    type Output = SecretShare;

    fn add(self, constant: FieldElement) -> SecretShare {
        SecretShare::new(self.value + constant)
    }
}

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

    #[test]
    fn test_share_recover() {
        let mut rng = RandState::new();
        let field = Field::new(101.into()); // 101 is prime
        let value = FieldElement::new(100.into(), Rc::<Field>::new(field));

        assert_eq!(value.clone(), LSS::recover(LSS::share(value, 10, &mut rng)));
    }

    #[test]
    fn test_share_splitting() {
        let mut rng = RandState::new();
        let field = Field::new(101.into()); // 101 is prime
        let value = FieldElement::new(100.into(), Rc::<Field>::new(field));

        // Share generates different shares each time
        assert_ne!(
            LSS::share(value.clone(), 10, &mut rng),
            LSS::share(value, 10, &mut rng)
        );
    }

    #[test]
    fn test_share_add() {
        let mut rng = RandState::new();
        let field = Rc::<Field>::new(Field::new(101.into())); // 101 is prime

        // setup
        let value1 = FieldElement::new(100.into(), field.clone());
        let value2 = FieldElement::new(100.into(), field);
        let shares1 = LSS::share(value1.clone(), 10, &mut rng);
        let shares2 = LSS::share(value2.clone(), 10, &mut rng);

        // test share addition
        let mut shares_sum: Vec<SecretShare> = Vec::new();
        for (share1, share2) in shares1.iter().zip(shares2.iter()) {
            shares_sum.push(share1.clone() + share2.clone());
        }

        // make sure adding shares results in the correct sum
        assert_eq!(value1 + value2, LSS::recover(shares_sum));
    }

    #[test]
    fn test_share_sub() {
        let mut rng = RandState::new();
        let field = Rc::<Field>::new(Field::new(101.into())); // 101 is prime

        // setup
        let value1 = FieldElement::new(100.into(), field.clone());
        let value2 = FieldElement::new(100.into(), field);
        let shares1 = LSS::share(value1.clone(), 10, &mut rng);
        let shares2 = LSS::share(value2.clone(), 10, &mut rng);

        // test share subtraction
        let mut shares_sum: Vec<SecretShare> = Vec::new();
        for (share1, share2) in shares1.iter().zip(shares2.iter()) {
            shares_sum.push(share1.clone() - share2.clone());
        }

        // assert that subtraction works
        assert_eq!(value1 - value2, LSS::recover(shares_sum));
    }

    #[test]
    fn test_share_constant_add() {
        let mut rng = RandState::new();
        let field = Rc::<Field>::new(Field::new(101.into())); // 101 is prime
        let value = FieldElement::new(100.into(), field.clone());

        // share the value
        let mut shares = LSS::share(value.clone(), 10, &mut rng);

        // add a constant to it
        let constant = FieldElement::new(5.into(), field);
        shares[0] = shares[0].clone() + constant.clone();

        // make sure the recovered shares are correct
        assert_eq!(value + constant, LSS::recover(shares));
    }

    #[test]
    fn test_share_constant_mul() {
        let mut rng = RandState::new();
        let field = Rc::<Field>::new(Field::new(101.into())); // 101 is prime
        let value = FieldElement::new(100.into(), field.clone());

        // share the value
        let mut shares = LSS::share(value.clone(), 10, &mut rng);

        // multiply all shares by a constant
        let constant = FieldElement::new(5.into(), field);
        for share in shares.iter_mut() {
            *share = (*share).clone() * constant.clone();
        }

        // ensure correct recovery
        assert_eq!(value * constant, LSS::recover(shares));
    }

    #[test]
    fn test_share_different_n() {
        let mut rng = RandState::new();
        let field = Field::new(101.into()); // 101 is prime
        let value = FieldElement::new(100.into(), Rc::<Field>::new(field));

        // share with 2-5 way splits and make sure the shares are recoverable
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
        let field = Field::new(101.into()); // 101 is prime
        let value = FieldElement::new(100.into(), Rc::<Field>::new(field));
        LSS::share(value, 1, &mut rng);
    }
}
