//! Spectrum implementation.
use crate::bytes::Bytes;
use rug::Integer;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops;
use std::sync::Arc;

/// mathematical group object
#[derive(Clone, Eq, PartialEq, Debug, Hash)]
pub struct Group {
    gen: Integer,     // generator for the group
    modulus: Integer, // group modulus
}

/// element within a group
#[derive(Clone, Eq, PartialEq, Debug, Hash)]
pub struct GroupElement {
    value: Integer,    // gen^k for some k
    group: Arc<Group>, // reference to the group the element resides in
}

impl Group {
    /// creates a new group object
    pub fn new(gen: Integer, modulus: Integer) -> Group {
        Group { gen, modulus }
    }

    pub fn element_from_bytes(self: &Arc<Group>, bytes: &Bytes) -> GroupElement {
        // TODO: find a less hacky way of doing this?
        let byte_str = hex::encode(bytes);
        let val = Integer::from_str_radix(&byte_str, 16).unwrap();
        GroupElement::new(val, self.clone())
    }
}

impl GroupElement {
    /// generates a new group element gen^v mod group.modulus
    pub fn new(v: Integer, group: Arc<Group>) -> GroupElement {
        let value = group.gen.clone().pow_mod(&v, &group.modulus).unwrap();
        GroupElement { value, group }
    }

    pub fn get_value(&self) -> Integer {
        self.value.clone()
    }

    pub fn exp(&self, pow: Integer) -> GroupElement {
        GroupElement {
            value: self
                .value
                .clone()
                .pow_mod(&pow, &self.group.modulus)
                .unwrap(),
            group: self.group.clone(),
        }
    }
}

/// applies the group operation on two elements
impl ops::Mul<GroupElement> for GroupElement {
    type Output = GroupElement;

    fn mul(self, other: GroupElement) -> GroupElement {
        assert_eq!(self.group, other.group);
        GroupElement::new(Integer::from(&self.value * &other.value), other.group)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_element_op() {
        let val1 = Integer::from(20);
        let val2 = Integer::from(10);
        let m = Integer::from(23);
        let g = Integer::from(2);
        let group = Arc::<Group>::new(Group::new(g, m));

        let elem1 = GroupElement::new(val1, group.clone());
        let elem2 = GroupElement::new(val2, group.clone());

        let expected = GroupElement::new(Integer::from(&elem1.value * &elem2.value), group);
        let actual = elem1 * elem2;

        assert_eq!(actual, expected);
    }
}
