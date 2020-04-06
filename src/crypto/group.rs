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
// PR to expose this as public within the library:
// https://github.com/zkcrypto/jubjub/pull/34
const JUBJUB_MODULUS: [u64; 4] = [
    0xd097_0e5e_d6f7_2cb7_u64,
    0xa668_2093_ccc8_1082_u64,
    0x0667_3b01_0134_3b00_u64,
    0x0e7d_b4ea_6533_afa9_u64,
];

// size of group elements in jubjbu
const JUBJUB_MODULUS_BYTES: usize = 32;

// max number of bytes that can be cast to a jubjub group element
const JUBJUB_MAX_CONVERT_BYTES: usize = 64;

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
        // need to remove all extra significant bits to ensure that
        // the value fits into a JUBJUB_MAX_CONVERT_BYTES byte array
        // see significant_digits:
        // https://docs.rs/rug/1.7.0/rug/struct.Integer.html#method.significant_digits
        let cutoff_bits: u32 = 8 * JUBJUB_MAX_CONVERT_BYTES as u32;
        let mut digits: [u8; JUBJUB_MAX_CONVERT_BYTES] = [0x0u8; JUBJUB_MAX_CONVERT_BYTES];
        Integer::from(value.keep_bits_ref(cutoff_bits)).write_digits(&mut digits, BYTE_ORDER);
        GroupElement(ECFieldElement::from_bytes_wide(&digits))
    }

    /// new element from little endian bytes
    pub fn element_from_bytes(bytes: Bytes) -> GroupElement {
        let mut bytes_arr: [u8; JUBJUB_MAX_CONVERT_BYTES] = [0; JUBJUB_MAX_CONVERT_BYTES];
        bytes_arr.copy_from_slice(bytes.as_ref());
        GroupElement(ECFieldElement::from_bytes_wide(&bytes_arr))
    }

    /// generates a new random group element
    pub fn rand_element() -> GroupElement {
        // generate enough random bytes to create a random element in the group
        let mut bytes = vec![0; JUBJUB_MAX_CONVERT_BYTES];
        thread_rng().fill_bytes(&mut bytes);
        Self::element_from_bytes(bytes.into())
    }

    /// generates a set of field elements in the elliptic curve field
    /// which are generators for the group (given that the group is of prime order)
    /// takes as input a random seed which deterministically generates [num] field elements
    pub fn generators(num: usize, seed: &AESSeed) -> Vec<GroupElement> {
        let prg = AESPRG::new(16, JUBJUB_MAX_CONVERT_BYTES * num);
        let rand_bytes: Vec<u8> = prg.eval(seed).into();

        //TODO: maybe use itertools::Itertools chunks?
        (0..num)
            .map(|i| {
                let chunk = rand_bytes
                    [i * JUBJUB_MAX_CONVERT_BYTES..(i + 1) * JUBJUB_MAX_CONVERT_BYTES]
                    .to_vec();
                Group::element_from_bytes(Bytes::from(chunk))
            })
            .collect()
    }

    pub fn order() -> Integer {
        // see JubJub elliptic curve modulus
        Integer::from_digits(&JUBJUB_MODULUS, BYTE_ORDER)
    }

    pub fn order_size_in_bytes() -> usize {
        JUBJUB_MODULUS_BYTES // size of the group elements
    }
}

impl GroupElement {
    pub fn pow(&self, pow: &Integer) -> GroupElement {
        // in EC group operation is addition, so exponentiation = multiplying
        GroupElement(self.0 * Group::new_element(pow).0)
    }
}

impl ops::Mul<GroupElement> for GroupElement {
    type Output = GroupElement;

    fn mul(self, rhs: GroupElement) -> GroupElement {
        GroupElement(self.0.add(&rhs.0))
    }
}

impl ops::Mul<&GroupElement> for GroupElement {
    type Output = GroupElement;

    fn mul(self, rhs: &GroupElement) -> GroupElement {
        GroupElement(self.0.add(&rhs.0))
    }
}

impl ops::MulAssign<&GroupElement> for GroupElement {
    fn mul_assign(&mut self, rhs: &GroupElement) {
        self.0 = self.0.add(&rhs.0);
    }
}

impl ops::MulAssign<GroupElement> for GroupElement {
    fn mul_assign(&mut self, rhs: GroupElement) {
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
    fn integer_512_bits() -> impl Strategy<Value = Integer> {
        any_with::<Bytes>(JUBJUB_MAX_CONVERT_BYTES.into())
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
            assert_eq!((x.clone() * y.clone()) * z.clone(), x * (y * z));
        }

        #[test]
        fn test_add_identity(element: GroupElement) {
            assert_eq!(element.clone() * Group::identity(), Group::identity() * element.clone());
            assert_eq!(element.clone() * Group::identity(), element.clone());
            assert_eq!(element.clone(), Group::identity() * element);

        }


        #[test]
        fn test_exp_prod(element: GroupElement, a in integer_512_bits(), b in integer_512_bits()) {
            let prod = a.clone() * b.clone();
            let expected = prod % Group::order();
            assert_eq!(element.pow(&a).pow(&b), element.pow(&expected))
        }

        #[test]
        fn test_sums_in_exponent(element: GroupElement, a in integer_512_bits(), b in integer_512_bits()) {
            let expected = a.clone() + b.clone() % Group::order();
            assert_eq!(element.pow(&a) * element.pow(&b), element.pow(&expected))
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
            seed2: AESSeed
        ) {
            prop_assume!(seed1 != seed2, "Different generators only for different seeds");
            assert_ne!(Group::generators(num, &seed1), Group::generators(num, &seed2));
        }
    }
}
