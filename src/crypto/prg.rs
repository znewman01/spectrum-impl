//! Spectrum implementation.
use crate::bytes::Bytes;
use crate::crypto::field::{Field, FieldElement};
use openssl::symm::{encrypt, Cipher};
use rand::prelude::*;
use std::sync::Arc;

const AES_SEED_SIZE: usize = 16; // 16 bytes for AES 128

pub trait PRG {
    type Seed;

    fn new_seed(&self) -> Self::Seed;
    fn eval(&self, seed: &Self::Seed, eval_size: usize) -> Bytes;
}

/// PRG uses AES to expand a seed to desired length
#[derive(Default, Clone, PartialEq, Debug, Copy)]
pub struct AESPRG {}

/// seed for a specific PRG
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct AESSeed {
    bytes: Bytes,
}

impl AESSeed {
    pub fn to_field_element(&self, field: Arc<Field>) -> FieldElement {
        field.from_bytes(&self.bytes)
    }
}

impl Into<Vec<u8>> for AESSeed {
    fn into(self) -> Vec<u8> {
        self.bytes.into()
    }
}

impl From<Vec<u8>> for AESSeed {
    fn from(other: Vec<u8>) -> Self {
        Self {
            bytes: other.into(),
        }
    }
}

impl AESPRG {
    pub fn new() -> Self {
        Default::default()
    }
}

impl PRG for AESPRG {
    type Seed = AESSeed;

    /// generates a new (random) seed for the given PRG
    fn new_seed(&self) -> AESSeed {
        // seed is just random bytes
        let mut key = vec![0; AES_SEED_SIZE];
        thread_rng().fill_bytes(&mut key);

        AESSeed {
            bytes: Bytes::from(key),
        }
    }

    /// evaluates the PRG on the given seed
    fn eval(&self, seed: &AESSeed, eval_size: usize) -> Bytes {
        assert!(
            eval_size >= AES_SEED_SIZE,
            "eval size must be at least the seed size"
        );

        // nonce set to zero: PRG eval should be deterministic
        let iv: [u8; 16] = [0; 16];

        // data is what AES will be "encrypting"
        // must be of size self.eval_size since we want the PRG
        // to expand to that size
        let data = vec![0; eval_size];

        // crt mode is fastest and ok for PRG
        let cipher = Cipher::aes_128_ctr();
        let mut ciphertext = encrypt(
            cipher,
            seed.bytes.as_ref(), // use seed bytes as the AES "key"
            Some(&iv),
            &data,
        )
        .unwrap();

        // truncate to correct expanded size
        ciphertext.truncate(eval_size);
        ciphertext.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashSet;
    use std::fmt::Debug;
    use std::ops::Range;

    const SIZES: Range<usize> = AES_SEED_SIZE..1000;

    impl Arbitrary for AESPRG {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            Just(AESPRG::new()).boxed()
        }
    }

    impl Arbitrary for AESSeed {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            prop::collection::vec(any::<u8>(), AES_SEED_SIZE)
                .prop_map(|data| AESSeed { bytes: data.into() })
                .boxed()
        }
    }

    fn run_test_prg_seed_random<P>(prg: P)
    where
        P: PRG,
        P::Seed: Eq + Debug,
    {
        assert_ne!(prg.new_seed(), prg.new_seed());
    }

    fn run_test_prg_eval_correct_size<P: PRG>(prg: P, seed: P::Seed, size: usize) {
        assert_eq!(prg.eval(&seed, size).len(), size);
    }

    fn run_test_prg_eval_deterministic<P: PRG>(prg: P, seed: P::Seed, size: usize) {
        assert_eq!(prg.eval(&seed, size), prg.eval(&seed, size));
    }

    fn run_test_prg_eval_random<P: PRG>(prg: P, seeds: &[P::Seed], size: usize) {
        #![allow(clippy::mutable_key_type)] // https://github.com/rust-lang/rust-clippy/issues/5043
        let results: HashSet<_> = seeds.iter().map(|s| prg.eval(s, size)).collect();
        assert_eq!(results.len(), seeds.len());
    }

    proptest! {
        #[test]
        fn test_aes_prg_seed_random(prg in any::<AESPRG>()) {
            run_test_prg_seed_random(prg);
        }

        #[test]
        fn test_aes_prg_eval_correct_size(prg in any::<AESPRG>(), seed in any::<AESSeed>(), size in SIZES) {
            run_test_prg_eval_correct_size(prg, seed, size);
        }

        #[test]
        fn test_aes_prg_eval_deterministic(prg in any::<AESPRG>(), seed in any::<AESSeed>(), size in SIZES) {
            run_test_prg_eval_deterministic(prg, seed, size);
        }

        #[test]
        fn test_aes_prg_eval_random(
            prg in any::<AESPRG>(),
            seeds in prop::collection::vec(any::<AESSeed>(), 10),
            size in SIZES
        ) {
            run_test_prg_eval_random(prg, &seeds, size);
        }
    }
}
