//! Spectrum implementation.
use rand::Rng;
use std::convert::AsRef;
use std::iter::FromIterator;
use std::ops;

#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Bytes(Vec<u8>);

impl Bytes {
    pub fn empty(len: usize) -> Bytes {
        vec![0; len].into()
    }

    pub fn random<R: Rng>(len: usize, rng: &mut R) -> Bytes {
        let mut len = len;
        let mut buf = Vec::<u8>::with_capacity(len);
        while len > 4096 {
            let mut chunk = [0u8; 4096];
            rng.fill(&mut chunk[..]);
            buf.extend(chunk.iter());
            len -= 4096;
        }
        let mut chunk = [0u8; 4096];
        rng.fill(&mut chunk[..]);
        buf.extend(chunk[0..len].iter());
        Bytes(buf)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(other: Vec<u8>) -> Self {
        Bytes(other)
    }
}

impl From<Bytes> for Vec<u8> {
    fn from(value: Bytes) -> Vec<u8> {
        value.0
    }
}

impl FromIterator<u8> for Bytes {
    fn from_iter<I: IntoIterator<Item = u8>>(iter: I) -> Self {
        iter.into_iter().collect::<Vec<u8>>().into()
    }
}

impl IntoIterator for Bytes {
    type Item = u8;
    type IntoIter = std::vec::IntoIter<u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl ops::BitOr<&Bytes> for Bytes {
    type Output = Bytes;

    fn bitor(self, rhs: &Bytes) -> Bytes {
        assert_eq!(self.len(), rhs.len());
        self.0
            .iter()
            .zip(rhs.0.iter())
            .map(|(x, y)| x | y)
            .collect()
    }
}

impl ops::BitOrAssign<&Bytes> for Bytes {
    fn bitor_assign(&mut self, rhs: &Bytes) {
        assert_eq!(self.len(), rhs.len());
        self.0
            .iter_mut()
            .zip(rhs.0.iter())
            .for_each(|(x, y)| *x |= y);
    }
}

impl ops::BitXor<&Bytes> for Bytes {
    type Output = Bytes;

    fn bitxor(self, rhs: &Bytes) -> Bytes {
        assert_eq!(self.len(), rhs.len());
        self.0
            .iter()
            .zip(rhs.0.iter())
            .map(|(x, y)| x ^ y)
            .collect()
    }
}

impl<A: AsRef<Bytes>> ops::BitXor<A> for Bytes {
    type Output = Bytes;

    fn bitxor(self, rhs: A) -> Bytes {
        self ^ rhs.as_ref()
    }
}

impl ops::BitXor<Bytes> for Bytes {
    type Output = Bytes;

    fn bitxor(self, rhs: Bytes) -> Bytes {
        self ^ &rhs
    }
}

impl ops::BitXorAssign<&Bytes> for Bytes {
    fn bitxor_assign(&mut self, rhs: &Bytes) {
        assert_eq!(self.len(), rhs.len());
        if self.len() > 100000 {
            let chunks_l = self.0.as_mut_slice().chunks_mut(128);
            let chunks_r = rhs.0.as_slice().chunks(128);
            chunks_l.zip(chunks_r).for_each(|(chunk_l, chunk_r)| {
                chunk_l.iter_mut().zip(chunk_r).for_each(|(l, r)| *l ^= r);
            });
        } else {
            self.0
                .iter_mut()
                .zip(rhs.0.iter())
                .for_each(|(x, y)| *x ^= y);
        }
    }
}

impl ops::BitXorAssign<Bytes> for Bytes {
    fn bitxor_assign(&mut self, rhs: Bytes) {
        *self ^= &rhs;
    }
}

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

#[cfg(any(test, feature = "testing"))]
impl Arbitrary for Bytes {
    type Parameters = prop::collection::SizeRange;
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(size: Self::Parameters) -> Self::Strategy {
        any_with::<Vec<u8>>((size, ()))
            .prop_map(Bytes::from)
            .boxed()
    }
}

#[cfg(any(test, feature = "testing"))]
pub fn bytes(len: usize) -> impl Strategy<Value = Bytes> {
    prop::collection::vec(any::<u8>(), len).prop_map(Bytes::from)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use rand::thread_rng;
    use std::ops::Range;

    const SIZE_RANGE: Range<usize> = 0..4097;

    fn is_all_zero(bytes: Bytes) -> bool {
        bytes.0.iter().all(|x| *x == 0)
    }

    proptest! {

        #[test]
        fn test_bytes_random_correct_size(size in SIZE_RANGE) {
            let bytes = Bytes::random(size, &mut thread_rng());
            assert_eq!(bytes.len(), size);
        }
        #[test]
        fn test_bytes_random_nonzero(size in SIZE_RANGE) {
            let mut rng = &mut thread_rng();
            let mut accum = Bytes::empty(size);
            // Pr[a given byte being zero] = 2^-8
            // ...a little high for testing: repeat until it's 2^-80
            for _ in 0..10 {
                let rand = Bytes::random(size, &mut rng);
                // if we OR, every bit that ever gets set in rand will stay set in accum
                accum |= &rand
            }
            prop_assert!(accum.0.iter().all(|x| *x != 0 ),
                    "Every byte should be non-zero sometimes.");
        }

        #[test]
        fn test_bytes_empty_correct_size(size in SIZE_RANGE) {
            let bytes = Bytes::empty(size);
            prop_assert_eq!(bytes.len(), size);
        }

        #[test]
        fn test_bytes_empty_zero(size in SIZE_RANGE) {
            let value = Bytes::empty(size);
            prop_assert!(is_all_zero(value),
                    "Every byte should be zero always.");
        }

        #[test]
        fn test_bytes_xor_zero(size in SIZE_RANGE) {
            let mut value = Bytes::random(size, &mut thread_rng());
            value ^= value.clone();
            prop_assert!(is_all_zero(value),
                    "XORing with self should give 0.");
        }
    }
}
