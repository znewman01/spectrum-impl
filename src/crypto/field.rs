//! Spectrum implementation.
use std::fmt::Debug;
use std::ops;
use std::rc::Rc;
use rug::{rand::RandState,Integer, integer::IsPrime};


/// prime order field
#[derive(Clone, PartialEq, Debug)]
pub struct Field {
    order: Integer,
}

/// element in a prime order field
#[derive(Clone, PartialEq, Debug)]
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
        if order.is_probably_prime(15) == IsPrime::No {
            panic!("field must have prime order!");
        }

        Field { order: order }
    }

    // generates a new random field element
    pub fn rand_element(self, rng: &mut RandState) -> FieldElement {
        
        // TODO: figure out how to generate a random value
        let random = self.order.clone().random_below(rng);

        FieldElement {
            value: random,
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

    pub fn field(self) -> Rc<Field> {
        self.field
    }
}

/// override + operation: want result.value = element1.value  + element2.value  mod field.order
impl ops::Add<FieldElement> for FieldElement {
    type Output = FieldElement;

    fn add(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);
        FieldElement::new(
            Integer::from(&self.value + &other.value) % &other.field.order.clone(),
            other.field,
        )
    }
}

/// override - operation: want result.vvalue = element1.value  + (-element2.value) mod field.order
impl ops::Sub<FieldElement> for FieldElement {
    type Output = FieldElement;

    fn sub(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);
        FieldElement::new(
            Integer::from(&self.value - &other.value) % &other.field.order.clone(),
            other.field,
        )
    }
}

/// override * operation: want result.value = element1.value * element2.value mod field.order
impl ops::Mul<FieldElement> for FieldElement {
    type Output = FieldElement;

    fn mul(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);
        FieldElement::new(
            Integer::from(&self.value * &other.value) % &other.field.order.clone(),
            other.field,
        )
    }
}

/// override += operation
impl ops::AddAssign<FieldElement> for FieldElement {
    fn add_assign(&mut self, other: FieldElement) {
        assert_eq!(self.field, other.field);
        *self = Self {
            value: self.value.clone() + other.value,
            field: other.field,
        };
    }
}

/// override -= operation
impl ops::SubAssign<FieldElement> for FieldElement {
    fn sub_assign(&mut self, other: FieldElement) {
        assert_eq!(self.field, other.field);
        *self = Self {
            value: self.value.clone() - other.value,
            field: other.field,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: additional tests:
    // 1) ==, != work as expected
    // 2) all these ops w/ different fields result in panic


    #[test]
    fn test_field_rand_element() {
        let p = Integer::from(23);
        let field = Field::new(p);

        let mut rng = RandState::new();
        assert_ne!(field.clone().rand_element(&mut rng), field.clone().rand_element(&mut rng));
    }

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

    #[test]
    #[should_panic]
    fn test_field_gen_non_prime() {
        Field::new(Integer::from(4));
    }
}
