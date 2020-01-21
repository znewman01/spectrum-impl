//! Spectrum implementation.
use rug::{integer::IsPrime, rand::RandState, Integer};
use std::fmt::Debug;
use std::ops;
use std::rc::Rc;

// mathematical field element
#[derive(Clone, PartialEq, Debug)]
pub struct Field {
    p: Integer,
}

pub struct FieldElement {
    v: Integer,
    field: std::rc::Rc<Field>,
}

impl Field {
    // generate new field element
    pub fn new(p: Integer) -> Field {
        assert_eq!(p.is_probably_prime(15), IsPrime::Yes);
        Field { p: p }
    }
}

impl FieldElement {
    // generates a new field element; v mod field.p
    pub fn new(v: Integer, field: std::rc::Rc<Field>) -> FieldElement {
        FieldElement {
            v: v % &field.p.clone(),
            field: field,
        }
    }

    // generates a new ranom field element
    pub fn rand(field: std::rc::Rc<Field>) -> FieldElement {
        let mut rand = RandState::new();
        FieldElement {
            v: field.p.clone().random_below(&mut rand),
            field: field,
        }
    }
}

// override + operation: want result.v = element1.v + element2.v mod field.p
impl ops::Add<FieldElement> for FieldElement {
    type Output = FieldElement;

    fn add(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);

        let res_ref = &self.v + &other.v;
        let res_mod = Integer::from(res_ref) % &other.field.p.clone();
        FieldElement {
            v: res_mod,
            field: other.field,
        }
    }
}

// override - operation: want result.v = element1.v + (-element2.v) mod field.p
impl ops::Sub<FieldElement> for FieldElement {
    type Output = FieldElement;

    fn sub(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);

        let res_ref = &self.v - &other.v;
        let res_mod = Integer::from(res_ref) % &other.field.p.clone();
        FieldElement {
            v: res_mod,
            field: other.field,
        }
    }
}

// override * operation: want result.v = element1.v * element2.v mod field.p
impl ops::Mul<FieldElement> for FieldElement {
    type Output = FieldElement;

    fn mul(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);

        let res_ref = &self.v * &other.v;
        let res_mod = Integer::from(res_ref) % &other.field.p.clone();
        FieldElement {
            v: res_mod,
            field: other.field,
        }
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
        let field = std::rc::Rc::<Field>::new(Field::new(p));

        let el1 = FieldElement::new(Integer::from(&val1), field.clone());
        let el2 = FieldElement::new(Integer::from(&val2), field.clone());
        let el_add = el1 + el2;
        let add_plain_ref = &val1 + &val2;
        assert_eq!(el_add.v, Integer::from(add_plain_ref) % field.p.clone());
    }

    #[test]
    fn test_field_element_sub() {
        let val1 = Integer::from(20);
        let val2 = Integer::from(10);
        let p = Integer::from(23);
        let field = std::rc::Rc::<Field>::new(Field::new(p));

        let el1 = FieldElement::new(Integer::from(&val1), field.clone());
        let el2 = FieldElement::new(Integer::from(&val2), field.clone());
        let el_sub = el1 - el2;
        let sub_plain_ref = &val1 - &val2;
        assert_eq!(el_sub.v, Integer::from(sub_plain_ref) % field.p.clone());
    }

    #[test]
    fn test_field_element_mul() {
        let val1 = Integer::from(20);
        let val2 = Integer::from(10);
        let p = Integer::from(23);
        let field = std::rc::Rc::<Field>::new(Field::new(p));

        let el1 = FieldElement::new(Integer::from(&val1), field.clone());
        let el2 = FieldElement::new(Integer::from(&val2), field.clone());
        let el_mul = el1 * el2;
        let mul_plain_ref = &val1 * &val2;
        assert_eq!(el_mul.v, Integer::from(mul_plain_ref) % field.p.clone());
    }
}
