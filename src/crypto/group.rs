//! Spectrum implementation.
use rug::Integer;
use std::fmt::Debug;
use std::ops;
use std::rc::Rc;

/// mathematical group object
#[derive(Clone, PartialEq, Debug)]
pub struct Group {
    gen: Integer, // generator for the group
    modulus: Integer, // group modulus
}

/// element within a group
#[derive(Clone, PartialEq, Debug)]
pub struct GroupElement {
    value: Integer, // gen^k for some k
    group: Rc<Group>, // reference to the group the element resides in
}

impl Group {
    /// creates a new group object
    pub fn new(gen: Integer, modulus: Integer) -> Group {
        Group { gen: gen, modulus: modulus }
    }
}

impl GroupElement {
    /// generates a new group element gen^v mod group.modulus
    pub fn new(v: Integer, group: Rc<Group>) -> GroupElement {
        let res = group.gen.clone().pow_mod(&v, &group.modulus).unwrap();
        GroupElement { value: res, group: group }
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
        let group = Rc::<Group>::new(Group::new(g, m));

        let elem1 = GroupElement::new(val1.clone(), group.clone());
        let elem2 = GroupElement::new(val2.clone(), group.clone());
       
        let expected = GroupElement::new(Integer::from(&val1 + &val2), group.clone());
        let actual = elem1 * elem2;

        assert_eq!(actual, expected);
    }
}
