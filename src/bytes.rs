//! Spectrum implementation.
use bytes::Bytes as OtherBytes;
use std::convert::AsRef;
use std::iter::FromIterator;
use std::ops;

#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Bytes(Vec<u8>);

impl Bytes {
    pub fn empty(len: usize) -> Bytes {
        vec![0; len].into()
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

impl Into<OtherBytes> for Bytes {
    fn into(self) -> OtherBytes {
        OtherBytes::from(self.0)
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(other: Vec<u8>) -> Self {
        Bytes(other)
    }
}

impl Into<Vec<u8>> for Bytes {
    fn into(self) -> Vec<u8> {
        self.0
    }
}

impl FromIterator<u8> for Bytes {
    fn from_iter<I: IntoIterator<Item = u8>>(iter: I) -> Self {
        iter.into_iter().collect::<Vec<u8>>().into()
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

impl ops::BitXorAssign<&Bytes> for Bytes {
    fn bitxor_assign(&mut self, rhs: &Bytes) {
        assert_eq!(self.len(), rhs.len());
        self.0
            .iter_mut()
            .zip(rhs.0.iter())
            .for_each(|(x, y)| *x ^= y);
    }
}
