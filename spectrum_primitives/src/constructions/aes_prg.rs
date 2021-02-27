use std::convert::TryFrom;

use derivative::Derivative;
use openssl::symm::{encrypt, Cipher};
use serde::{Deserialize, Serialize};

use crate::bytes::Bytes;
use crate::prg::PRG;

pub const SEED_SIZE: usize = 16; // in bytes

/// PRG uses AES to expand a seed to desired length
#[derive(Clone, PartialEq, Copy, Serialize, Deserialize, Derivative)]
#[derivative(Debug)]
pub struct AESPRG {
    eval_size: usize,
    #[serde(skip, default = "Cipher::aes_128_ctr")]
    #[derivative(Debug = "ignore")]
    cipher: Cipher,
}

/// seed for AES-based PRG
#[derive(Default, Clone, PartialEq, Eq, Debug, Hash)]
pub struct AESSeed {
    bytes: Bytes,
}

/// evaluation type for AES-based PRG
impl AESSeed {
    pub fn random() -> Self {
        use rand::prelude::*;
        let mut rand_seed_bytes = vec![0; SEED_SIZE];
        thread_rng().fill_bytes(&mut rand_seed_bytes);
        AESSeed::try_from(rand_seed_bytes).expect("Correct seed size")
    }

    pub fn bytes(self) -> Bytes {
        self.bytes
    }
}

impl Into<Vec<u8>> for AESSeed {
    fn into(self) -> Vec<u8> {
        self.bytes.into()
    }
}

impl TryFrom<Vec<u8>> for AESSeed {
    type Error = ();

    fn try_from(other: Vec<u8>) -> Result<Self, ()> {
        if other.len() != SEED_SIZE {
            return Err(());
        }
        Ok(Self {
            bytes: other.into(),
        })
    }
}

impl AESPRG {
    pub fn new(eval_size: usize) -> Self {
        assert!(
            SEED_SIZE <= eval_size,
            "eval size must be at least the seed size"
        );

        AESPRG {
            eval_size,
            cipher: Cipher::aes_128_ctr(),
        }
    }
}

// Implementation of an AES-based PRG
impl PRG for AESPRG {
    type Seed = AESSeed;
    type Output = Bytes;

    /// generates a new (random) seed for the given PRG
    fn new_seed(&self) -> AESSeed {
        AESSeed::random()
    }

    fn output_size(&self) -> usize {
        self.eval_size
    }

    /// evaluates the PRG on the given seed
    fn eval(&self, seed: &AESSeed) -> Self::Output {
        // nonce set to zero: PRG eval should be deterministic
        let iv: [u8; 16] = [0; 16];

        // data is what AES will be "encrypting"
        // must be of size self.eval_size since we want the PRG
        // to expand to that size
        let data = vec![0; self.eval_size];

        // crt mode is fastest and ok for PRG
        let mut ciphertext = encrypt(
            self.cipher,
            seed.bytes.as_ref(), // use seed bytes as the AES "key"
            Some(&iv),
            &data,
        )
        .unwrap();

        ciphertext.truncate(self.eval_size);
        ciphertext.into()
    }

    fn null_output(&self) -> Bytes {
        Bytes::empty(self.eval_size)
    }
}

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

#[cfg(any(test, feature = "testing"))]
impl Arbitrary for AESPRG {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use std::ops::Range;
        const SIZES: Range<usize> = 16..1000; // in bytes
        SIZES
            .prop_map(|output_size| AESPRG::new(output_size))
            .boxed()
    }
}

#[cfg(any(test, feature = "testing"))]
impl Arbitrary for AESSeed {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        prop::collection::vec(any::<u8>(), SEED_SIZE)
            .prop_map(AESSeed::try_from)
            .prop_map(Result::unwrap)
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    check_prg!(AESPRG);
    check_dpf!(crate::dpf::TwoKeyDpf<AESPRG>);
}
