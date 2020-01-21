//! Spectrum implementation.
use rug::Integer;
use std::fmt::Debug;
use std::ops;
use std::rc::Rc;

// mathematical group object
// g: generator of the group
// p: group modulus
#[derive(Clone, PartialEq, Debug)]
pub struct Group {
    pub g: Integer, // generator
    pub p: Integer, // modulus
}

// element within a group
// v:  g^k for some k
// gp: reference to the group the element resides in
#[derive(Clone)]
pub struct GroupElement {
    v: Integer,
    gp: std::rc::Rc<Group>,
}

impl Group {
    // create a new group with generator g and modulus p
    pub fn new(g: Integer, p: Integer) -> Group {
        Group { g: g, p: p }
    }
}

impl GroupElement {
    // generate new group element g^v mod p
    pub fn new(v: Integer, gp: std::rc::Rc<Group>) -> GroupElement {
        let res = match gp.g.clone().pow_mod(&v, &gp.p) {
            Ok(power) => power,
            Err(_) => unreachable!(),
        };

        GroupElement { v: res, gp: gp }
    }
}

// applies the group operation on two elements
impl ops::Mul<GroupElement> for GroupElement {
    type Output = GroupElement;

    fn mul(self, other: GroupElement) -> GroupElement {
        assert_eq!(self.gp, other.gp);

        let res_ref = &self.v * &other.v;
        let res_mod = Integer::from(res_ref) % &other.gp.p.clone();
        GroupElement {
            v: res_mod,
            gp: other.gp,
        }
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
        let gp = std::rc::Rc::<Group>::new(Group::new(g, m));

        let val1_exp = match gp.g.clone().pow_mod(&val1, &gp.p) {
            Ok(power) => power,
            Err(_) => unreachable!(),
        };

        let val2_exp = match gp.g.clone().pow_mod(&val2, &gp.p) {
            Ok(power) => power,
            Err(_) => unreachable!(),
        };

        let mul_plain_ref = &val1_exp * &val2_exp;
        let expected = Integer::from(mul_plain_ref) % gp.p.clone();

        let el1 = GroupElement::new(val1.clone(), gp.clone());
        let el2 = GroupElement::new(val2.clone(), gp.clone());
        let el_op = el1 * el2;

        assert_eq!(el_op.v, expected);
    }
}
