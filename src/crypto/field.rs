//! Spectrum implementation.
use rug::{integer::IsPrime, rand::RandState, Integer};
use std::fmt::Debug;
use std::ops;
use std::rc::Rc;

/// prime order field
#[derive(Clone, PartialEq, Debug)]
pub struct Field {
    order: Integer,
}

/// element in a prime order field
pub struct FieldElement {
    value: Integer,
    field: Rc<Field>,
}

impl Field {
    /// generate new field of prime order
    /// order must be a prime
    pub fn new(order: Integer) -> Field {
        // probability of failure is negligible in k, suggested to set k=15
        // which is the default used by Rust https://docs.rs/rug/1.6.0/rug/struct.Integer.html
        assert_eq!(order.is_probably_prime(15), IsPrime::Yes);
        Field { order: order }
    }

    // generates a new random field element
    pub fn rand_element(self, rng: Option<RandState>) -> FieldElement {
        let mut rand = rng.unwrap_or(RandState::new());
        FieldElement {
            value: self.order.clone().random_below(&mut rand),
            field: Rc::<Field>::new(self),
        }
    }
}

impl FieldElement {
    /// generates a new field element; value mod field.order
    pub fn new(v: Integer, field: Rc<Field>) -> FieldElement {
        FieldElement {
            value: v % &field.order.clone(),
            field: field,
        }
    }
}

/// override + operation: want result.value = element1.value  + element2.value  mod field.order
impl ops::Add<FieldElement> for FieldElement {
    type Output = FieldElement;

    fn add(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);
        FieldElement::new(Integer::from(&self.value + &other.value) % &other.field.order.clone(), other.field)
    }
}

/// override - operation: want result.vvalue = element1.value  + (-element2.value) mod field.order
impl ops::Sub<FieldElement> for FieldElement {
    type Output = FieldElement;

    fn sub(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);
        FieldElement::new(Integer::from(&self.value - &other.value) % &other.field.order.clone(), other.field)
    }
}

/// override * operation: want result.value = element1.value * element2.value mod field.order
impl ops::Mul<FieldElement> for FieldElement {
    type Output = FieldElement;

    fn mul(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);
        FieldElement::new(Integer::from(&self.value * &other.value) % &other.field.order.clone(), other.field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_element_add() {
        let val1 = Integer::from(20);
        let val2 = Integer::from(10);
        let p = Integer::from(23);
        let field = Rc::<Field>::new(Field::new(p));

        let elem1 = FieldElement::new(val1.clone(), field.clone());
        let elem2 = FieldElement::new(val2.clone(), field.clone());
        let acutal = (elem1 + elem2).value;
        let expected = Integer::from(&val1 + &val2) % field.order.clone();
        assert_eq!(acutal, expected);
    }

    #[test]
    fn test_field_element_sub() {
        let val1 = Integer::from(20);
        let val2 = Integer::from(10);
        let p = Integer::from(23);
        let field = Rc::<Field>::new(Field::new(p));

        let elem1 = FieldElement::new(Integer::from(&val1), field.clone());
        let elem2 = FieldElement::new(Integer::from(&val2), field.clone());
        let acutal = (elem1 - elem2).value;
        let expected = Integer::from(&val1 - &val2) % field.order.clone();
        assert_eq!(acutal, expected);
    }

    #[test]
    fn test_field_element_mul() {
        let val1 = Integer::from(20);
        let val2 = Integer::from(10);
        let p = Integer::from(23);
        let field = Rc::<Field>::new(Field::new(p));

        let elem1 = FieldElement::new(Integer::from(&val1), field.clone());
        let elem2 = FieldElement::new(Integer::from(&val2), field.clone());
        let actual = (elem1 * elem2).value;
        let expected = Integer::from(&val1 * &val2) % field.order.clone();
        assert_eq!(actual, expected);
    }
}
