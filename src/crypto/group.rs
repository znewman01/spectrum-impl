//! Spectrum implementation.
use crate::bytes::Bytes;
use crate::crypto::prg::{aes::AESSeed, aes::AESPRG, PRG};
use jubjub::Fr as ECFieldElement; // elliptic curve field
use rand::prelude::*;
use rug::{integer::Order, Integer};
use std::fmt::Debug;
use std::hash::Hash;
use std::hash::Hasher;
use std::ops;

const BYTE_ORDER: Order = Order::LsfLe;

// see jubjub::Fr for details
// TODO: if we continue using JubJub, we should send them a PR to make this public.
const JUBJUB_MODULUS: [u64; 4] = [
    0xd097_0e5e_d6f7_2cb7_u64,
    0xa668_2093_ccc8_1082_u64,
    0x0667_3b01_0134_3b00_u64,
    0x0e7d_b4ea_6533_afa9_u64,
];

/// mathematical group object
#[derive(Default, Clone, Eq, PartialEq, Debug)]
pub struct Group(ECFieldElement); // generator for the multiplicative group

/// element within a group
#[derive(Clone, Eq, Debug)]
pub struct GroupElement(ECFieldElement);

impl Group {
    /// identity element in the elliptic curve field
    pub fn identity() -> GroupElement {
        GroupElement(ECFieldElement::zero())
    }

    /// creates a new group element from an integer
    pub fn new_element(value: &Integer) -> GroupElement {
        assert!(
            value.significant_digits::<u8>() <= 64,
            "new element should be at most 64 bytes"
        );

        let mut digits: [u8; 64] = [0x0u8; 64];
        value.write_digits(&mut digits, BYTE_ORDER);
        GroupElement(ECFieldElement::from_bytes_wide(&digits))
    }

    /// new element from little endian bytes
    pub fn element_from_bytes(bytes: Bytes) -> GroupElement {
        assert!(
            bytes.len() <= 64,
            "cannot cast more than 64 bytes into a group element"
        );

        let mut bytes_arr: [u8; 64] = [0; 64];
        bytes_arr.copy_from_slice(bytes.as_ref());
        GroupElement(ECFieldElement::from_bytes_wide(&bytes_arr))
    }

    /// generates a new random group element
    pub fn rand_element() -> GroupElement {
        // generate enough random bytes to create a random element in the group
        let mut bytes = vec![0; 32];
        thread_rng().fill_bytes(&mut bytes);
        Self::element_from_bytes(bytes.into())
    }

    /// generates a set of field elements in the elliptic curve field
    /// which are generators for the group (given that the group is of prime order)
    /// takes as input a random seed which deterministically generates [num] field elements
    pub fn generators(num: usize, seed: &AESSeed) -> Vec<GroupElement> {
        let prg = AESPRG::new(16, 64 * num);
        let rand_bytes: Vec<u8> = prg.eval(seed).into();

        //TODO: maybe use itertools::Itertools chunks?
        (0..num)
            .map(|i| {
                let chunk = rand_bytes[i * 64..(i + 1) * 64].to_vec();
                Group::element_from_bytes(Bytes::from(chunk))
            })
            .collect()
    }

    pub fn order() -> Integer {
        // see JubJub elliptic curve modulus
        Integer::from_digits(&JUBJUB_MODULUS, BYTE_ORDER)
    }

    pub fn order_size_in_bytes() -> usize {
        32 // 32 bytes is the size of the group elements
    }
}

impl GroupElement {
    pub fn pow(&self, pow: &Integer) -> GroupElement {
        // in EC group opertion is addition, so exponentiation = multiplying
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
    }
}

// need to implement PartialEq when implementing Hash...
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

    // need to generate 512-bit integers to ensure all operations
    // "wrap around" the group order during testing
    pub fn integer_512_bits() -> impl Strategy<Value = Integer> {
        any_with::<Bytes>(63.into())
            .prop_map(|bytes| Integer::from_digits(&bytes.as_ref(), BYTE_ORDER))
    }

    impl Arbitrary for GroupElement {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            integer_512_bits()
                .prop_map(move |value| Group::new_element(&value))
                .boxed()
        }
    }

    #[test]
    fn group_is_of_prime_order() {
        assert_ne!(Group::order().is_probably_prime(15), IsPrime::No)
    }

    proptest! {

        #[test]
        fn test_associative(x: GroupElement, y: GroupElement, z: GroupElement) {
            assert_eq!((x.clone() ^ y.clone()) ^ z.clone(), x ^ (y ^ z));
        }

        #[test]
        fn test_add_identity(element: GroupElement) {
            assert_eq!(element.clone() ^ Group::identity(), Group::identity() ^ element);
        }


        #[test]
        fn test_exp_prod(element: GroupElement, a in integer_512_bits(), b in integer_512_bits()) {
            let prod = a.clone() * b.clone();
            let prod_reduced = prod % Group::order();
            assert_eq!(element.pow(&a).pow(&b), element.pow(&prod_reduced))
        }

        #[test]
        fn test_sums_in_exponent(element: GroupElement, a in integer_512_bits(), b in integer_512_bits()) {
            let sum = a.clone() + b.clone();
            assert_eq!(element.pow(&a)^element.pow(&b), element.pow(&sum))
        }

        #[test]
        fn test_generators_deterministic(
            num in NUM_GROUP_GENERATORS,
            seed: AESSeed) {
            assert_eq!(Group::generators(num, &seed), Group::generators(num, &seed));
        }

        #[test]
        fn test_generators_different_seeds_different_generators(
            num in NUM_GROUP_GENERATORS,
            seed1: AESSeed,
            seed2: AESSeed) {
            prop_assume!(seed1 != seed2, "Different generators only for different seeds");
            assert_ne!(Group::generators(num, &seed1), Group::generators(num, &seed2));
        }
    }
}
