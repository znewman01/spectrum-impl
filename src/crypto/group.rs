//! Spectrum implementation.
use jubjub::Fr as ECFieldElement; // elliptic curve field
use openssl::symm::{encrypt, Cipher};
use rand::prelude::*;
use rug::{integer::Order, Integer};
use std::fmt::Debug;
use std::hash::Hash;
use std::hash::Hasher;
use std::ops;

const BYTE_ORDER: Order = Order::LsfLe;
const JUBJUB_MODULUS: [u64; 4] = [
    0xd097_0e5e_d6f7_2cb7_u64,
    0xa668_2093_ccc8_1082_u64,
    0x0667_3b01_0134_3b00_u64,
    0x0e7d_b4ea_6533_afa9_u64,
];

/// mathematical group object
#[derive(Default, Clone, Eq, PartialEq, Debug)]
pub struct Group(ECFieldElement); // generator for the multipliocative group

// TODO(sss): implement hash for GroupElement
/// element within a group
#[derive(Clone, Eq, Debug)]
pub struct GroupElement(ECFieldElement);

impl Group {
    /// additive identity in the elliptic curve field
    pub fn order_byte_size() -> usize {
        32 // 32 bytes is the size of the group elements
    }

    /// additive identity in the elliptic curve field
    pub fn additive_identity() -> GroupElement {
        GroupElement(ECFieldElement::zero())
    }

    pub fn multiplicative_identity() -> GroupElement {
        GroupElement(ECFieldElement::one())
    }
    /// creates a new group element from an integer
    pub fn new_element(value: &Integer) -> GroupElement {
        let mut digits: [u8; 64] = [0x0u8; 64];
        let value_reduced = value % Group::order();
        value_reduced.write_digits(&mut digits, BYTE_ORDER);
        GroupElement(ECFieldElement::from_bytes_wide(&digits))
    }

    /// new element from little endian bytes
    pub fn element_from_bytes(bytes: [u8; 64]) -> GroupElement {
        GroupElement(ECFieldElement::from_bytes_wide(&bytes))
    }

    /// generates a new random group element
    pub fn rand_element() -> GroupElement {
        // generate enough random bytes to create a random element in the group
        let mut rand_bytes: [u8; 64] = [0; 64];
        thread_rng().fill_bytes(&mut rand_bytes);
        Self::element_from_bytes(rand_bytes)
    }

    /// generates a set of field elements in the elliptic curve field
    /// which are generators for the group (given that the group is of prime order)
    /// takes as input a random seed which deterministically generates [num] field elements
    pub fn generators(num: usize, seed: &[u8; 16]) -> Vec<GroupElement> {
        // nonce set to zero: PRG eval should be deterministic
        let iv: [u8; 16] = [0; 16];
        let data = vec![0; 64 * num];
        let cipher = Cipher::aes_128_ctr();
        let ciphertext = encrypt(
            cipher,
            seed, // use seed bytes as the AES "key"
            Some(&iv),
            &data,
        )
        .unwrap();

        (0..num)
            .map(|i| {
                let mut bytes_arr: [u8; 64] = [0; 64];
                bytes_arr.copy_from_slice(&ciphertext[i * 64..(i + 1) * 64]);
                Group::element_from_bytes(bytes_arr)
            })
            .collect()
    }

    pub fn order() -> Integer {
        // see JubJub elliptic curve modulus
        Integer::from_digits(&JUBJUB_MODULUS, BYTE_ORDER)
    }
}

impl GroupElement {
    pub fn pow(&self, pow: &Integer) -> GroupElement {
        // in EC group law is addition, so exponentiation = multiplying
        GroupElement(self.0 * Group::new_element(pow).0)
    }
}

impl ops::BitXor<GroupElement> for GroupElement {
    type Output = GroupElement;

    fn bitxor(self, rhs: GroupElement) -> GroupElement {
        GroupElement(self.0.add(&rhs.0))
    }
}

impl ops::BitXor<&GroupElement> for GroupElement {
    type Output = GroupElement;

    fn bitxor(self, rhs: &GroupElement) -> GroupElement {
        GroupElement(self.0.add(&rhs.0))
    }
}

impl ops::BitXorAssign<&GroupElement> for GroupElement {
    fn bitxor_assign(&mut self, rhs: &GroupElement) {
        self.0 = self.0.add(&rhs.0);
    }
}

impl ops::BitXorAssign<GroupElement> for GroupElement {
    fn bitxor_assign(&mut self, rhs: GroupElement) {
        self.0 = self.0.add(&rhs.0);
    }
}

impl Hash for GroupElement {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.0.to_bytes().hash(state);
        state.finish();
    }
}

impl PartialEq for GroupElement {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::Bytes;
    use proptest::prelude::*;
    use rug::integer::IsPrime;
    use std::ops::Range;
    const NUM_GROUP_GENERATORS: Range<usize> = 1..500;

    pub fn integers() -> impl Strategy<Value = Integer> {
        any_with::<Bytes>(64.into())
            .prop_map(|bytes| Integer::from_digits(&bytes.as_ref(), BYTE_ORDER))
    }

    impl Arbitrary for GroupElement {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            integers()
                .prop_map(move |value| Group::new_element(&value))
                .boxed()
        }
    }

    pub fn group_element_pairs() -> impl Strategy<Value = (GroupElement, GroupElement)> {
        (any::<GroupElement>(), any::<GroupElement>())
    }

    pub fn integer_triplet() -> impl Strategy<Value = (Integer, Integer, Integer)> {
        (
            any_with::<Bytes>(64.into())
                .prop_map(|bytes| Integer::from_digits(&bytes.as_ref(), BYTE_ORDER)),
            any_with::<Bytes>(64.into())
                .prop_map(|bytes| Integer::from_digits(&bytes.as_ref(), BYTE_ORDER)),
            any_with::<Bytes>(64.into())
                .prop_map(|bytes| Integer::from_digits(&bytes.as_ref(), BYTE_ORDER)),
        )
    }

    // Pair of field elements *in the same field*
    fn group_element_triplet() -> impl Strategy<Value = (GroupElement, GroupElement, GroupElement)>
    {
        (
            any::<GroupElement>(),
            any::<GroupElement>(),
            any::<GroupElement>(),
        )
    }

    #[test]
    fn group_is_of_prime_order() {
        assert_ne!(Group::order().is_probably_prime(15), IsPrime::No)
    }

    proptest! {

        #[test]
        fn test_op_commutative((x, y) in group_element_pairs()) {
            assert_eq!(x.clone() ^ y.clone(), y ^ x);
        }

        #[test]
        fn test_op_associative((x, y, z) in group_element_triplet()) {
            assert_eq!((x.clone() ^ y.clone()) ^ z.clone(), x ^ (y ^ z));
        }

        #[test]
        fn test_add_identity(element in any::<GroupElement>()) {
            assert_eq!(element.clone() ^ Group::additive_identity(), Group::additive_identity() ^ element);
        }


        #[test]
        fn test_exp_prod(el in any::<GroupElement>(), (a, b, c) in integer_triplet()) {
            let prod = a.clone() * b.clone() * c.clone();
            assert_eq!(el.pow(&a).pow(&b).pow(&c), el.pow(&prod))
        }

        #[test]
        fn test_sums_in_exponent(el in any::<GroupElement>(), (a, b, c) in integer_triplet()) {
            let sum = a.clone() + b.clone() + c.clone();
            assert_eq!(el.pow(&a)^el.pow(&b)^el.pow(&c), el.pow(&sum))
        }

        #[test]
        fn test_generators_deterministic(
            num in NUM_GROUP_GENERATORS,
            seed in any::<[u8; 16]>()) {
            assert_eq!(Group::generators(num, &seed), Group::generators(num, &seed));
        }

        #[test]
        fn test_generators_different_seeds_different_generators(
            num in NUM_GROUP_GENERATORS,
            seed1 in any::<[u8; 16]>(),
            seed2 in any::<[u8; 16]>()) {
            assert_ne!(Group::generators(num, &seed1), Group::generators(num, &seed2));
        }
    }
}
