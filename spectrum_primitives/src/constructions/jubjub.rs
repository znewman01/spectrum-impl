use std::convert::{TryFrom, TryInto};
use std::hash::{Hash, Hasher};
use std::iter::Sum;
use std::ops;

use ::group::GroupEncoding;
use jubjub::{Fr, SubgroupPoint};
use rug::{integer::Order, Integer};

use crate::algebra::{Field, Group, Monoid, SpecialExponentMonoid};
use crate::bytes::Bytes;
use crate::constructions::aes_prg::{AesPrg, AesSeed};
use crate::util::Sampleable;

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

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

/// A CurvePoint representing an exponent in the elliptic curve group.
#[derive(Eq, Debug, Clone)]
pub struct CurvePoint {
    inner: SubgroupPoint,
}

// // TODO: want From<Bytes> to be: encoding a message. And TryFrom<Vec<u8>> to be: serializing a CurvePoint.
// impl From<CurvePoint> for Bytes {
//     fn from(value: CurvePoint) -> Bytes {
//         Bytes::from(Vec::from((&value.inner).to_bytes()))
//     }
// }
//
// impl From<Bytes> for CurvePoint {
//     fn from(value: CurvePoint) -> Bytes {
//         Bytes::from(Vec::from((&value.inner).to_bytes()))
//     }
// }

impl From<CurvePoint> for Vec<u8> {
    fn from(value: CurvePoint) -> Self {
        Vec::from((&value.inner).to_bytes())
    }
}

impl TryFrom<Vec<u8>> for CurvePoint {
    type Error = ();

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        let bytes = value.try_into().map_err(|_| ())?;
        let result: Option<SubgroupPoint> = SubgroupPoint::from_bytes(&bytes).into();
        result.ok_or(()).map(Into::into)
    }
}

impl Sampleable for CurvePoint {
    type Seed = AesSeed;

    fn sample() -> Self {
        use rand::thread_rng;
        <SubgroupPoint as ::group::Group>::random(&mut thread_rng()).into()
    }

    fn sample_many_from_seed(seed: &Self::Seed, n: usize) -> Vec<Self> {
        let _ = seed;
        let _ = n;
        todo!()
    }
}

impl Monoid for CurvePoint {
    fn zero() -> Self {
        use ::group::Group;
        SubgroupPoint::identity().into()
    }
}

impl Group for CurvePoint {
    fn order() -> Integer {
        Integer::from_digits(&MODULUS, BYTE_ORDER)
    }
}

impl ops::Add for CurvePoint {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        (self.inner + rhs.inner).into()
    }
}

impl ops::AddAssign for CurvePoint {
    fn add_assign(&mut self, rhs: Self) {
        self.inner += rhs.inner;
    }
}

impl ops::Neg for CurvePoint {
    type Output = Self;

    fn neg(self) -> Self {
        (-self.inner).into()
    }
}

impl ops::Sub for CurvePoint {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        (self.inner - rhs.inner).into()
    }
}

impl Sum for CurvePoint {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> CurvePoint {
        let mut total = <Self as Monoid>::zero();
        iter.for_each(|value| total += value);
        total
    }
}

// Boilerplate: conversions etc.
impl From<SubgroupPoint> for CurvePoint {
    fn from(inner: SubgroupPoint) -> Self {
        CurvePoint { inner }
    }
}

impl Hash for CurvePoint {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.to_bytes().hash(state);
    }
}

impl PartialEq for CurvePoint {
    fn eq(&self, rhs: &CurvePoint) -> bool {
        self.inner == rhs.inner
    }
}

#[cfg(any(test, feature = "testing"))]
pub(crate) fn subgroup_points() -> impl Strategy<Value = SubgroupPoint> {
    use ::group::Group as _;
    any::<u8>().prop_map(|mut exp| {
        let g = SubgroupPoint::generator();
        let mut p = g;
        loop {
            // Exponentiation by squaring
            // Err, multiplication by doubling, but same idea.
            if exp % 2 == 1 {
                p += g;
            }
            exp /= 2;
            if exp <= 1 {
                break;
            }
            p = p.double();
        }
        p
    })
}

#[cfg(any(test, feature = "testing"))]
impl Arbitrary for CurvePoint {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        subgroup_points().prop_map(CurvePoint::from).boxed()
    }
}

/// A scalar representing an exponent in the elliptic curve group.
#[derive(Eq, Debug, Clone)]
pub struct Scalar {
    inner: Fr,
}

impl Monoid for Scalar {
    fn zero() -> Self {
        Fr::zero().into()
    }
}

impl Group for Scalar {
    fn order() -> Integer {
        Integer::from_digits(&MODULUS, BYTE_ORDER)
    }
}

impl Field for Scalar {
    fn mul_invert(&self) -> Self {
        self.inner.invert().unwrap().into()
    }

    fn one() -> Self {
        Fr::one().into()
    }
}

impl ops::Add for Scalar {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        (self.inner + rhs.inner).into()
    }
}

impl ops::AddAssign for Scalar {
    fn add_assign(&mut self, rhs: Self) {
        self.inner += &rhs.inner;
    }
}

impl ops::Neg for Scalar {
    type Output = Self;

    fn neg(self) -> Self {
        (-self.inner).into()
    }
}

impl ops::Sub for Scalar {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        (self.inner - rhs.inner).into()
    }
}

impl ops::Mul for Scalar {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        (self.inner * rhs.inner).into()
    }
}

impl Sum for Scalar {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Scalar {
        let mut total = <Self as Monoid>::zero();
        iter.for_each(|value| total += value);
        total
    }
}

// Boilerplate: conversions etc.
impl From<Fr> for Scalar {
    fn from(inner: Fr) -> Self {
        Scalar { inner }
    }
}

impl From<&Integer> for Scalar {
    fn from(value: &Integer) -> Self {
        use std::cmp::Ordering;
        let reduced = if value.cmp0() == Ordering::Less {
            Self::order() - (Integer::from(-value) % Self::order())
        } else {
            value % Self::order()
        };

        let mut digits: [u8; MODULUS_BYTES] = [0x0u8; MODULUS_BYTES];
        reduced.write_digits(&mut digits, BYTE_ORDER);
        Fr::from_bytes(&digits).unwrap().into()
    }
}

impl From<Integer> for Scalar {
    fn from(value: Integer) -> Self {
        Self::from(&value)
    }
}

impl From<Scalar> for Bytes {
    fn from(value: Scalar) -> Bytes {
        Bytes::from(value.inner.to_bytes().to_vec())
    }
}

impl TryFrom<Bytes> for Scalar {
    type Error = String;

    fn try_from(bytes: Bytes) -> Result<Self, Self::Error> {
        let len = bytes.len();
        if len <= MODULUS_BYTES {
            let mut bytes_arr: [u8; MODULUS_BYTES] = [0; MODULUS_BYTES];
            bytes_arr[..len].copy_from_slice(bytes.as_ref());
            Option::<Fr>::from(Fr::from_bytes(&bytes_arr))
                .map(Scalar::from)
                .ok_or_else(|| "Converting from bytes failed.".to_string())
        } else if len == 64 {
            let mut bytes_arr: [u8; 64] = [0; 64];
            bytes_arr.copy_from_slice(bytes.as_ref());
            Ok(Scalar::from(Fr::from_bytes_wide(&bytes_arr)))
        } else {
            Err(format!("invalid byte length {}", bytes.len()))
        }
    }
}

impl TryFrom<Vec<u8>> for Scalar {
    type Error = ();

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Option::<Fr>::from(Fr::from_bytes(&value.try_into().map_err(|_| ())?))
            .map(Scalar::from)
            .ok_or(())
    }
}

impl From<Scalar> for Vec<u8> {
    fn from(value: Scalar) -> Vec<u8> {
        value.inner.to_bytes().into()
    }
}

impl Hash for Scalar {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.to_bytes().hash(state);
    }
}

impl PartialEq for Scalar {
    fn eq(&self, rhs: &Scalar) -> bool {
        self.inner == rhs.inner
    }
}

#[cfg(any(test, feature = "testing"))]
pub(crate) fn jubjubs() -> impl Strategy<Value = Fr> {
    proptest::collection::vec(any::<u8>(), 64)
        .prop_map(|v| Fr::from_bytes_wide(v.as_slice().try_into().unwrap()))
}

#[cfg(any(test, feature = "testing"))]
impl Arbitrary for Scalar {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        jubjubs().prop_map(Scalar::from).boxed()
    }
}

impl Sampleable for Scalar {
    type Seed = AesSeed;
    /// identity element in the elliptic curve field

    /// generates a new random group element
    fn sample() -> Self {
        use rand::thread_rng;
        <Fr as ::ff::Field>::random(&mut thread_rng()).into()
    }

    fn sample_many_from_seed(seed: &Self::Seed, n: usize) -> Vec<Self> {
        use crate::prg::Prg;
        if n == 0 {
            return vec![];
        }
        let prg = AesPrg::new((MODULUS_BYTES - 1) * n);
        let rand_bytes: Vec<u8> = prg.eval(seed).into();

        //TODO: maybe use itertools::Itertools chunks?
        (0..n)
            .map(|i| {
                let mut chunk =
                    rand_bytes[i * (MODULUS_BYTES - 1)..(i + 1) * (MODULUS_BYTES - 1)].to_vec();
                chunk.push(0);
                Scalar::try_from(Bytes::from(chunk))
                    .expect("chunk size chosen s.t. always valid element")
            })
            .collect()
    }
}

impl SpecialExponentMonoid for CurvePoint {
    type Exponent = Scalar;

    fn pow(&self, exp: Self::Exponent) -> Self {
        (self.inner * exp.inner).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dpf::MultiKeyDpf;
    use crate::prg::GroupPrg;

    check_group_laws!(CurvePoint);
    // check_sampleable!(CurvePoint);
    check_field_laws!(Scalar);
    check_sampleable!(Scalar);
    check_shareable!(Scalar);
    check_linearly_shareable!(Scalar);
    // check_roundtrip!(Bytes, Into::<CurvePoint>::into, Bytes::from, bytes_to_point);
    check_roundtrip!(
        CurvePoint,
        Into::<Vec<u8>>::into,
        |x| CurvePoint::try_from(x).unwrap(),
        point_to_vec_u8
    );
    check_prg!(GroupPrg<CurvePoint>);
    check_seed_homomorphic_prg!(GroupPrg<CurvePoint>);

    check_dpf!(MultiKeyDpf<GroupPrg<CurvePoint>>);

    check_roundtrip!(
        Scalar,
        Into::<Vec<u8>>::into,
        |x| Scalar::try_from(x).unwrap(),
        scalar_to_vec_u8
    );

    use crate::ElementVector;
    check_roundtrip!(
        ElementVector<CurvePoint>,
        Into::<Vec<u8>>::into,
        |d| ElementVector::<CurvePoint>::try_from(d).unwrap(),
        element_vector
    );
}
