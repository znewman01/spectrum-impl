//! Spectrum implementation.
use crate::bytes::Bytes;
use crate::proto;
use rug::rand::RandState;
use rug::{integer::IsPrime, integer::Order, Integer};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::ops;

const BYTE_ORDER: Order = Order::LsfLe;

// NOTE: can't use From/Into due to Rust orphaning rules. Define an extension trait?
// TODO(zjn): more efficient data format?
fn parse_u128(data: &str) -> u128 {
    data.parse::<u128>().unwrap()
}

fn emit_integer(value: &u128) -> String {
    value.to_string()
}

/// prime order field
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub struct Field {
    order: u128,
}

impl From<u128> for Field {
    fn from(value: u128) -> Field {
        Field::new(value)
    }
}

impl From<proto::Integer> for Field {
    fn from(msg: proto::Integer) -> Field {
        parse_u128(msg.data.as_ref()).into()
    }
}

impl Into<proto::Integer> for Field {
    fn into(self) -> proto::Integer {
        proto::Integer {
            data: emit_integer(&self.order),
        }
    }
}

/// element in a prime order field
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub struct FieldElement {
    value: u128,
    field: Field,
}

impl Field {
    /// generate new field of prime order
    /// order must be a prime
    pub fn new(order: u128) -> Field {
        // probability of failure is negligible in k, suggested to set k=15
        // which is the default used by Rust https://docs.rs/rug/1.6.0/rug/struct.Integer.html
        if Integer::from(order).is_probably_prime(15) == IsPrime::No {
            panic!("field must have prime order!");
        }

        Field { order }
    }

    pub fn zero(&self) -> FieldElement {
        FieldElement {
            value: 0,
            field: *self,
        }
    }

    pub fn new_element(&self, value: u128) -> FieldElement {
        FieldElement::new(value, *self)
    }

    // generates a new random field element
    pub fn rand_element(&self, rng: &mut RandState) -> FieldElement {
        let rand: Integer = Integer::from(self.order).random_below_ref(rng).into();
        FieldElement {
            value: rand.to_u128().unwrap(),
            field: *self,
        }
    }

    pub fn from_proto(&self, msg: proto::Integer) -> FieldElement {
        FieldElement::new(parse_u128(msg.data.as_ref()), *self)
    }

    pub fn element_from_bytes(&self, bytes: &Bytes) -> FieldElement {
        let val = Integer::from_digits(bytes.as_ref(), BYTE_ORDER);
        self.new_element(val.to_u128().unwrap())
    }
}

impl FieldElement {
    // TODO: move reduce_modulo to Field::new_element and remove
    /// generates a new field element; value mod field.order
    pub fn new(v: u128, field: Field) -> FieldElement {
        FieldElement {
            value: v % field.order,
            field,
        }
    }

    pub fn field(&self) -> Field {
        self.field
    }

    pub fn get_value(&self) -> u128 {
        self.value
    }
}

impl Into<Bytes> for FieldElement {
    fn into(self) -> Bytes {
        Bytes::from(Integer::from(self.value).to_digits(Order::LsfLe))
    }
}

impl Into<proto::Integer> for FieldElement {
    fn into(self) -> proto::Integer {
        proto::Integer {
            data: emit_integer(&self.value),
        }
    }
}

// adds two u128 values together and reduces modulo the provided modulus
// if an overflow occurs, the operation is performed using rug::Integer
// and then converted back to u128
fn add_mod(a: u128, b: u128, modulus: u128) -> u128 {
    match a.checked_add(b) {
        Some(result) => result % modulus,
        None => ((Integer::from(a) + Integer::from(b)) % Integer::from(modulus))
            .to_u128()
            .unwrap(),
    }
}

// multiplies two u128 values together and reduces modulo the provided modulus
// if an overflow occurs, the operation is performed using rug::Integer
// and then converted back to u128
fn mul_mod(a: u128, b: u128, modulus: u128) -> u128 {
    match a.checked_mul(b) {
        Some(result) => result % modulus,
        None => ((Integer::from(a) * Integer::from(b)) % Integer::from(modulus))
            .to_u128()
            .unwrap(),
    }
}

/// override + operation: want result.value = element1.value  + element2.value  mod field.order
impl ops::Add<FieldElement> for FieldElement {
    type Output = FieldElement;

    fn add(self, other: FieldElement) -> FieldElement {
        assert_eq!(self.field, other.field);

        FieldElement::new(
            add_mod(self.value, other.value, self.field.order),
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
            add_mod(self.value, (-other).value, self.field.order),
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
            mul_mod(self.value, other.value, self.field.order),
            other.field,
        )
    }
}

/// override * operation: want result.value = element1.value * element2.value mod field.order
impl ops::Mul<u128> for FieldElement {
    type Output = FieldElement;

    fn mul(self, scalar: u128) -> FieldElement {
        FieldElement::new(mul_mod(self.value, scalar, self.field.order), self.field)
    }
}

/// override negation of field element
impl ops::Neg for FieldElement {
    type Output = FieldElement;

    fn neg(self) -> FieldElement {
        FieldElement::new(self.field.order - self.value, self.field)
    }
}

/// override += operation
impl ops::AddAssign<FieldElement> for FieldElement {
    fn add_assign(&mut self, other: FieldElement) {
        assert_eq!(self.field, other.field);
        *self = Self {
            value: add_mod(self.value, other.value, self.field.order),
            field: other.field,
        };
    }
}

impl ops::SubAssign<FieldElement> for FieldElement {
    fn sub_assign(&mut self, other: FieldElement) {
        assert_eq!(self.field, other.field);
        *self = Self {
            value: add_mod(self.value, (-other).value, self.field.order),
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

    pub fn integers() -> impl Strategy<Value = u128> {
        any::<u128>()
    }

    pub fn prime_integers() -> impl Strategy<Value = u128> {
        integers().prop_map(|value| {
            let prime = Integer::next_prime(value.into());
            prime.to_u128().unwrap()
        })
    }

    impl Arbitrary for Field {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            prime_integers().prop_map(Field::from).boxed()
        }
    }

    impl Arbitrary for FieldElement {
        type Parameters = Option<Field>;
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(field: Self::Parameters) -> Self::Strategy {
            match field {
                Some(field) => integers()
                    .prop_map(move |value| field.new_element(value))
                    .boxed(),
                None => (integers(), any::<Field>())
                    .prop_map(|(value, field)| field.new_element(value))
                    .boxed(),
            }
        }
    }

    // Pair of field elements *in the same field*
    pub fn field_element_pairs() -> impl Strategy<Value = (FieldElement, FieldElement)> {
        any::<Field>().prop_flat_map(|field| {
            (
                any_with::<FieldElement>(Some(field)),
                any_with::<FieldElement>(Some(field)),
            )
        })
    }

    // Pair of field elements *in the same field*
    fn field_element_triples() -> impl Strategy<Value = (FieldElement, FieldElement, FieldElement)>
    {
        any::<Field>().prop_flat_map(|field| {
            (
                any_with::<FieldElement>(Some(field)),
                any_with::<FieldElement>(Some(field)),
                any_with::<FieldElement>(Some(field)),
            )
        })
    }

    // Several field elements, *all in the same field*
    pub fn field_element_vecs(num: usize) -> impl Strategy<Value = Vec<FieldElement>> {
        any::<Field>().prop_flat_map(move |field| {
            prop::collection::vec(integers().prop_map(move |v| field.new_element(v)), num)
        })
    }

    proptest! {
        #[test]
        fn test_field_rand_element_not_deterministic(field: Field) {
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

    proptest! {

    #[test]
    fn test_field_element_bytes_rt(element: FieldElement) {
        prop_assert_eq!(
            element.field.element_from_bytes(&element.into()),
            element
        );
      }
    }

    proptest! {

        #[test]
        fn test_add_commutative((x, y) in field_element_pairs()) {
            assert_eq!(x + y, y + x);
        }
        #[test]
        fn test_add_associative((x, y, z) in field_element_triples()) {
            assert_eq!((x + y) + z, x + (y + z));
        }

        #[test]
        fn test_add_sub_inverses((x, y) in field_element_pairs()) {
            assert_eq!(x - y, x + (-y));
        }

        #[test]
        fn test_add_inverse(x in any::<FieldElement>()) {
            assert_eq!(x.field().zero(), x + (-x));
        }

        #[test]
        fn test_mul_commutative((x, y) in field_element_pairs()) {
            assert_eq!(x * y, y * x);
        }

        #[test]
        fn test_mul_associative((x, y, z) in field_element_triples()) {
            assert_eq!((x * y) * z, x * (y * z));
        }

        #[test]
        fn test_distributive((x, y, z) in field_element_triples()) {
            assert_eq!(x * (y + z), (x * y) + (x * z));
        }

        #[test]
        #[should_panic]
        fn test_add_in_different_fields_fails(a: FieldElement, b: FieldElement) {
            prop_assume!(a.field.order != b.field.order, "Fields should not be equal");
            a + b
        }


        #[test]
        #[should_panic]
        fn test_prod_in_different_fields_fails(a: FieldElement, b: FieldElement) {
            prop_assume!(a.field.order != b.field.order, "Fields should not be equal");
            a * b
        }


        #[test]
        fn test_equality((a, b) in field_element_pairs()) {
            let eq = a == b && a.value == b.value && a.field.order == b.field.order;
            let neq = a != b && (a.value != b.value || a.field.order != b.field.order);
            assert!(neq || eq);
        }

    }

    #[test]
    #[should_panic]
    fn test_field_must_be_prime() {
        Field::new(4);
    }
}
