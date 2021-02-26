use std::convert::TryFrom;
use std::hash::{Hash, Hasher};
use std::iter::Sum;
use std::ops;

use jubjub::Fr as Scalar;
use rug::{integer::Order, Integer};

use crate::algebra::{Field, Group};
use crate::bytes::Bytes;
use crate::constructions::aes_prg::{AESSeed, AESPRG};
use crate::util::Sampleable;

// see jubjub::Fr for details
// PR to expose this as public within the library:
// https://github.com/zkcrypto/jubjub/pull/34
const MODULUS: [u64; 4] = [
    0xd097_0e5e_d6f7_2cb7_u64,
    0xa668_2093_ccc8_1082_u64,
    0x0667_3b01_0134_3b00_u64,
    0x0e7d_b4ea_6533_afa9_u64,
];

// size of group elements in jubjub
pub const MODULUS_BYTES: usize = 32;
const BYTE_ORDER: Order = Order::LsfLe;

/// A scalar representing a point in an elliptic curve subgroup.
#[derive(Eq, PartialEq, Debug, Clone)]
pub struct Point {
    inner: Scalar,
}

impl Group for Point {
    fn order() -> Integer {
        Integer::from_digits(&MODULUS, BYTE_ORDER)
    }

    fn zero() -> Self {
        Scalar::zero().into()
    }
}

impl Field for Point {
    fn mul_invert(&self) -> Self {
        self.inner.invert().unwrap().into()
    }

    fn one() -> Self {
        Scalar::one().into()
    }
}

impl ops::Add for Point {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        (self.inner + &rhs.inner).into()
    }
}

impl ops::AddAssign for Point {
    fn add_assign(&mut self, rhs: Self) {
        self.inner += &rhs.inner;
    }
}

impl ops::Neg for Point {
    type Output = Self;

    fn neg(self) -> Self {
        (-self.inner).into()
    }
}

impl ops::Sub for Point {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        (self.inner - rhs.inner).into()
    }
}

impl ops::Mul<Integer> for Point {
    type Output = Self;

    fn mul(self, rhs: Integer) -> Self {
        // in EC group operation is addition, so exponentiation = multiplying
        (self.inner * Self::from(rhs).inner).into()
    }
}

impl ops::Mul for Point {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        (self.inner * &rhs.inner).into()
    }
}

impl Sum for Point {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Point {
        let mut total = <Self as Group>::zero();
        iter.for_each(|value| total += value);
        total
    }
}

// Boilerplate: conversions etc.
impl From<Scalar> for Point {
    fn from(inner: Scalar) -> Self {
        Point { inner }
    }
}

impl From<&Integer> for Point {
    fn from(value: &Integer) -> Self {
        use std::cmp::Ordering;
        let reduced = if value.cmp0() == Ordering::Less {
            Self::order() - (Integer::from(-value) % Self::order())
        } else {
            value % Self::order()
        };

        let mut digits: [u8; MODULUS_BYTES] = [0x0u8; MODULUS_BYTES];
        reduced.write_digits(&mut digits, BYTE_ORDER);
        Scalar::from_bytes(&digits).unwrap().into()
    }
}

impl From<Integer> for Point {
    fn from(value: Integer) -> Self {
        Self::from(&value)
    }
}

impl Into<Bytes> for Point {
    fn into(self) -> Bytes {
        Bytes::from(self.inner.to_bytes().to_vec())
    }
}

impl TryFrom<Bytes> for Point {
    type Error = &'static str;

    fn try_from(bytes: Bytes) -> Result<Self, Self::Error> {
        match bytes.len() {
            MODULUS_BYTES => {
                let mut bytes_arr: [u8; MODULUS_BYTES] = [0; MODULUS_BYTES];
                bytes_arr.copy_from_slice(bytes.as_ref());
                Option::<Scalar>::from(Scalar::from_bytes(&bytes_arr))
                    .map(Point::from)
                    .ok_or("Converting from bytes failed.")
            }
            // 31 => {
            //     let mut bytes_arr: [u8; MODULUS_BYTES - 1] = [0; MODULUS_BYTES - 1];
            //     bytes_arr.copy_from_slice(bytes.as_ref());
            //     Scalar::from_bytes(&bytes_arr)
            // }
            64 => {
                let mut bytes_arr: [u8; 64] = [0; 64];
                bytes_arr.copy_from_slice(bytes.as_ref());
                Ok(Point::from(Scalar::from_bytes_wide(&bytes_arr)))
            }
            _ => {
                panic!("uh oh");
            }
        }
    }
}

impl Hash for Point {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.to_bytes().hash(state);
    }
}

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;
#[cfg(any(test, feature = "testing"))]
pub(crate) fn jubjubs() -> impl Strategy<Value = Scalar> {
    use std::convert::TryInto;
    proptest::collection::vec(any::<u8>(), 64)
        .prop_map(|v| Scalar::from_bytes_wide(v.as_slice().try_into().unwrap()))
}
#[cfg(any(test, feature = "testing"))]
impl Arbitrary for Point {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        jubjubs().prop_map(Point::from).boxed()
    }
}

impl Sampleable for Point {
    type Seed = AESSeed;
    /// identity element in the elliptic curve field

    /// generates a new random group element
    fn sample() -> Self {
        use rand::{thread_rng, RngCore};
        // generate enough random bytes to create a random element in the group
        let mut bytes = vec![0; MODULUS_BYTES * 2];
        thread_rng().fill_bytes(&mut bytes);
        Point::try_from(Bytes::from(bytes)).expect("chunk size chosen s.t. always valid element")
    }

    fn sample_many_from_seed(seed: &Self::Seed, n: usize) -> Vec<Self> {
        use crate::prg::PRG;
        if n == 0 {
            return vec![];
        }
        let prg = AESPRG::new((MODULUS_BYTES - 1) * n);
        let rand_bytes: Vec<u8> = prg.eval(seed).into();

        //TODO: maybe use itertools::Itertools chunks?
        (0..n)
            .map(|i| {
                let mut chunk =
                    rand_bytes[i * (MODULUS_BYTES - 1)..(i + 1) * (MODULUS_BYTES - 1)].to_vec();
                chunk.push(0);
                Point::try_from(Bytes::from(chunk))
                    .expect("chunk size chosen s.t. always valid element")
            })
            .collect()
    }

    // /// generates a set of field elements in the elliptic curve field
    // /// which are generators for the group (given that the group is of prime order)
    // /// takes as input a random seed which deterministically generates [num] field elements
    // fn generators(num: usize, seed: &AESSeed) -> Vec<GroupElement> {
    // }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lss::{LinearlyShareable, Shareable};
    use crate::prg::{GroupPRG, GroupPrgSeed, SeedHomomorphicPRG, PRG};

    check_group_laws!(Point);
    check_field_laws!(Point);
    check_sampleable!(Point);
    check_shareable!(Point);
    check_linearly_shareable!(Point);
    check_roundtrip!(
        Point,
        Into::<Bytes>::into,
        |x| Point::try_from(x).unwrap(),
        point_to_bytes
    );
    check_prg!(GroupPRG<Point>);
    check_seed_homomorphic_prg!(GroupPRG<Point>);
    check_group_laws!(GroupPrgSeed<Point>, prg_seed_group_laws);
}
