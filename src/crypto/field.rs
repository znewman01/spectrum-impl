//! Spectrum implementation.
use crate::bytes::Bytes;
use rug::{integer::IsPrime, rand::RandState, Integer};
use std::cmp::Ordering;
use std::fmt::Debug;
use std::ops;
use std::sync::Arc;

// TODO(zjn): move the Arc<> into the field; implement Arbitrary
/// prime order field
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct Field {
    order: Integer,
}

impl From<Integer> for Field {
    fn from(value: Integer) -> Field {
        Field::new(value)
    }
}

/// element in a prime order field
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct FieldElement {
    value: Integer,
    field: Arc<Field>,
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

        Field { order }
    }

    pub fn zero(self: &Arc<Field>) -> FieldElement {
        FieldElement {
            value: 0.into(),
            field: self.clone(),
        }
    }

    pub fn new_element(self: &Arc<Field>, value: Integer) -> FieldElement {
        FieldElement::new(value, self.clone())
    }

    // generates a new random field element
    pub fn rand_element(self: &Arc<Field>, rng: &mut RandState) -> FieldElement {
        // TODO: figure out how to generate a random value
        let random = self.order.clone().random_below(rng);

        FieldElement {
            value: random,
            field: self.clone(),
        }
    }

    pub fn from_bytes(self: &Arc<Field>, bytes: &Bytes) -> FieldElement {
        // TODO: fix this
        let byte_str = hex::encode(bytes);
        let val = Integer::from_str_radix(&byte_str, 16).unwrap();
        FieldElement::new(val, self.clone())
    }
}

impl FieldElement {
    /// generates a new field element; value mod field.order
    pub fn new(v: Integer, field: Arc<Field>) -> FieldElement {
        FieldElement {
            value: reduce_modulo(v, field.order.clone()),
            field,
        }
    }

    pub fn field(&self) -> Arc<Field> {
        self.field.clone()
    }
}

// perform modulo reducation after a field operation.
// note: different from % given that reduce_modulo compares
// to zero rather than just take the remainder.
fn reduce_modulo(v: Integer, order: Integer) -> Integer {
    if v.cmp0() == Ordering::Less {
        (order.clone() + v) % order
    } else {
        v % order
    }
}

/// override + operation: want result.value = element1.value  + element2.value  mod field.order
impl ops::Add<FieldElement> for FieldElement {
    type Output = FieldElement;

    fn add(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);

        let mut value = self.value + other.value;

        // perform modulo reduction after adding
        if value >= self.field.order {
            value -= self.field.order.clone()
        }

        FieldElement::new(value, other.field)
    }
}

/// override - operation: want result.vvalue = element1.value  + (-element2.value) mod field.order
impl ops::Sub<FieldElement> for FieldElement {
    type Output = FieldElement;

    fn sub(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);
        let mut value = self.value - other.value;

        // perform modulo reduction after subtracting
        if value >= self.field.order {
            value -= self.field.order.clone()
        }

        FieldElement::new(value, other.field)
    }
}

/// override * operation: want result.value = element1.value * element2.value mod field.order
impl ops::Mul<FieldElement> for FieldElement {
    type Output = FieldElement;

    fn mul(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);
        FieldElement::new(
            reduce_modulo(self.value * &other.value, other.field.order.clone()),
            other.field,
        )
    }
}

/// override * operation: want result.value = element1.value * element2.value mod field.order
impl ops::Mul<u8> for FieldElement {
    type Output = FieldElement;

    fn mul(self, scalar: u8) -> FieldElement {
        FieldElement::new(
            reduce_modulo(self.value * scalar, self.field.order.clone()),
            self.field,
        )
    }
}

/// override negation of field element
impl ops::Neg for FieldElement {
    type Output = FieldElement;

    fn neg(self) -> FieldElement {
        FieldElement::new(
            reduce_modulo(-self.value, self.field.order.clone()),
            self.field,
        )
    }
}

/// override += operation
impl ops::AddAssign<FieldElement> for FieldElement {
    fn add_assign(&mut self, other: FieldElement) {
        assert_eq!(self.field, other.field);
        *self = Self {
            value: reduce_modulo(self.value.clone() + other.value, other.field.order.clone()),
            field: other.field,
        };
    }
}

impl ops::SubAssign<FieldElement> for FieldElement {
    fn sub_assign(&mut self, other: FieldElement) {
        assert_eq!(self.field, other.field);
        *self = Self {
            value: reduce_modulo(self.value.clone() - other.value, other.field.order.clone()),
            field: other.field,
        };
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashSet;
    use std::iter::repeat_with;

    pub fn integers() -> impl Strategy<Value = Integer> {
        (0..1000).prop_map(Integer::from)
    }

    pub fn prime_integers() -> impl Strategy<Value = Integer> {
        integers().prop_map(Integer::next_prime)
    }

    pub fn fields() -> impl Strategy<Value = Arc<Field>> {
        prime_integers().prop_map(Field::from).prop_map(Arc::new)
    }

    impl Arbitrary for FieldElement {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            (fields(), integers())
                .prop_map(|(field, value)| field.new_element(value))
                .boxed()
        }
    }

    // Several field elements, *all in the same field*
    pub fn field_element_vecs(num: usize) -> impl Strategy<Value = Vec<FieldElement>> {
        fields().prop_flat_map(move |field| {
            prop::collection::vec(integers().prop_map(move |v| field.new_element(v)), num)
        })
    }

    // TODO: additional tests:
    // 1) ==, != work as expected
    // 2) all these ops w/ different fields result in panic

    proptest! {
        #[test]
        fn test_field_rand_element_not_deterministic(field in fields()) {
            let mut rng = RandState::new();
            let elements: HashSet<_> = repeat_with(|| field.rand_element(&mut rng))
                .take(10)
                .collect();
            assert!(
                elements.len() > 1,
                "Many random elements should not all be the same."
            );
        }
    }

    #[test]
    fn test_field_element_add() {
        let val1 = Integer::from(10);
        let val2 = Integer::from(20);
        let p = Integer::from(23);
        let field = Arc::new(Field::new(p));

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
        let field = Arc::new(Field::new(p));

        let elem1 = FieldElement::new(Integer::from(&val1), field.clone());
        let elem2 = FieldElement::new(Integer::from(&val2), field.clone());
        let acutal = (elem1 - elem2).value;
        let expected = Integer::from(&val1 - &val2) % field.order.clone();
        assert_eq!(acutal, expected);
    }

    #[test]
    fn test_field_element_mul() {
        let val1 = Integer::from(10);
        let val2 = Integer::from(20);
        let p = Integer::from(23);
        let field = Arc::new(Field::new(p));

        let elem1 = FieldElement::new(Integer::from(&val1), field.clone());
        let elem2 = FieldElement::new(Integer::from(&val2), field.clone());
        let actual = (elem1 * elem2).value;
        let expected = Integer::from(&val1 * &val2) % field.order.clone();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_field_element_neg() {
        let val = Integer::from(20);
        let p = Integer::from(23);
        let field = Arc::new(Field::new(p));

        let elem1 = FieldElement::new(Integer::from(&val), field.clone());
        let actual = -elem1;
        let expected = FieldElement::new(field.order.clone() - val, field);
        assert_eq!(actual, expected);
    }

    #[test]
    #[should_panic]
    fn test_field_gen_non_prime() {
        Field::new(Integer::from(4));
    }
}
