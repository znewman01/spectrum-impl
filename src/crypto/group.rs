//! Spectrum implementation.
use jubjub::Fr;
use openssl::symm::{encrypt, Cipher};
use rand::prelude::*;
use rug::{integer::Order, Integer};
use std::fmt::Debug;
use std::hash::Hash;
use std::hash::Hasher;
use std::ops;

const BYTE_ORDER: Order = Order::LsfLe;

/// mathematical group object
#[derive(Default, Clone, Eq, PartialEq, Debug)]
pub struct Group(Fr); // generator for the multipliocative group

// TODO(sss): implement hash for GroupElement
/// element within a group
#[derive(Clone, Eq, Debug)]
pub struct GroupElement(Fr);

impl Group {
    pub fn identity() -> GroupElement {
        GroupElement(Fr::zero())
    }

    pub fn new_element(value: Integer) -> GroupElement {
        let val_mod = value % &Group::order();
        let mut digits: [u8; 32] = [0x0u8; 32];
        val_mod.write_digits(&mut digits, BYTE_ORDER);
        GroupElement(Fr::from_bytes(&digits).unwrap())
    }

    pub fn new_element_from_bytes(bytes: [u8; 64]) -> GroupElement {
        GroupElement(Fr::from_bytes_wide(&bytes))
    }

    // generates a new random field element
    pub fn rand_element() -> GroupElement {
        // generate enough random bytes to create a random element in the group
        // size of group is obtained via modulus.significant_digits
        let mut rand_bytes: [u8; 64] = [0; 64];
        thread_rng().fill_bytes(&mut rand_bytes);
        Self::new_element_from_bytes(rand_bytes)
    }

    // generates a new random field element
    pub fn deterministic_generators(num: usize, seed: &[u8; 16]) -> Vec<GroupElement> {
        // generate determinisitc random bytes and convert to [num] generators

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
                Group::new_element_from_bytes(bytes_arr)
            })
            .collect()
    }

    pub fn order() -> Integer {
        // see JubJub eliptic curve modulus
        let bytes = [
            0xd097_0e5e_d6f7_2cb7_u64,
            0xa668_2093_ccc8_1082_u64,
            0x0667_3b01_0134_3b00_u64,
            0x0e7d_b4ea_6533_afa9_u64,
        ];

        Integer::from_digits(&bytes, BYTE_ORDER)
    }
}

impl GroupElement {
    pub fn exp(&self, pow: &Integer) -> GroupElement {
        let pow_mod = Integer::from(pow % &Group::order());
        GroupElement(self.0.mul(&Group::new_element(pow_mod).0))
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
    use proptest::prelude::*;
    use rug::integer::IsPrime;
    use std::ops::Range;
    const MAX_USIZE: usize = usize::max_value();
    const NUM_GROUP_GENERATORS: Range<usize> = 1..500;

    pub fn integers() -> impl Strategy<Value = Integer> {
        (0..MAX_USIZE).prop_map(Integer::from)
    }

    impl Arbitrary for GroupElement {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            integers()
                .prop_map(move |value| Group::new_element(value))
                .boxed()
        }
    }

    pub fn group_element_pairs() -> impl Strategy<Value = (GroupElement, GroupElement)> {
        (any::<GroupElement>(), any::<GroupElement>())
    }

    pub fn integer_triplet() -> impl Strategy<Value = (Integer, Integer, Integer)> {
        (
            (0..MAX_USIZE).prop_map(Integer::from),
            (0..MAX_USIZE).prop_map(Integer::from),
            (0..MAX_USIZE).prop_map(Integer::from),
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
        fn test_op_identity((x, y) in group_element_pairs()) {
            assert_eq!(x.clone() ^ y.clone() ^ Group::identity(), x ^ y);
        }

        #[test]
        fn test_exp_prod_in_exponent((a, b, c) in integer_triplet()) {
            let el = Group::rand_element();
            let prod = Integer::from(&a*&b) * c.clone();
            assert_eq!(el.exp(&a).exp(&b).exp(&c), el.exp(&prod))
        }


        #[test]
        fn test_op_mod_in_exponent(val in (0..MAX_USIZE).prop_map(Integer::from)) {
            let el = Group::rand_element();
            let modulo = Group::order();
            let val_pow = val.pow_mod(&Integer::from(4), &(modulo.clone()*2)).unwrap();
            let val_pow_reduced = Integer::from(&val_pow % &modulo);
            assert_eq!(el.exp(&val_pow), el.exp(&val_pow_reduced))
        }


        #[test]
        fn test_op_sums_in_exponent((a, b, c) in integer_triplet()) {
            let el = Group::rand_element();
            let modulo = Group::order();
            let a_pow = a.pow_mod(&Integer::from(4), &modulo).unwrap();
            let b_pow = b.pow_mod(&Integer::from(4), &modulo).unwrap();
            let c_pow = c.pow_mod(&Integer::from(4), &modulo).unwrap();
            let sum = Integer::from(&a_pow + &b_pow) + c_pow.clone();
            assert_eq!(el.exp(&a_pow)^el.exp(&b_pow)^el.exp(&c_pow), el.exp(&sum))
        }

        #[test]
        fn test_deterministic_generators(num in NUM_GROUP_GENERATORS) {
            let mut rand_seed: [u8; 16] = [0; 16];
            thread_rng().fill_bytes(&mut rand_seed);
            assert_eq!(Group::deterministic_generators(num, &rand_seed), Group::deterministic_generators(num, &rand_seed));
        }

        #[test]
        fn test_deterministic_generators_different_seeds(num in NUM_GROUP_GENERATORS) {
            let mut rand_seed_1: [u8; 16] = [0; 16];
            let mut rand_seed_2: [u8; 16] = [0; 16];
            thread_rng().fill_bytes(&mut rand_seed_1);
            thread_rng().fill_bytes(&mut rand_seed_2);
            assert_ne!(Group::deterministic_generators(num, &rand_seed_1), Group::deterministic_generators(num, &rand_seed_2));
        }
    }
}
