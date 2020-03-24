//! Spectrum implementation.
use crate::bytes::Bytes;
use crate::crypto::field::{Field, FieldElement};
use openssl::symm::{encrypt, Cipher};
use rand::prelude::*;
use std::fmt::Debug;
use std::hash::Hash;

pub trait PRG {
    type Seed;
    type Output;

    fn new_seed(&self) -> Self::Seed;
    fn eval(&self, seed: &Self::Seed) -> Self::Output;
    fn null_output(&self) -> Self::Output;
}

/// PRG uses AES to expand a seed to desired length
#[derive(Default, Clone, PartialEq, Debug, Copy)]
pub struct AESPRG {
    seed_size: usize,
    eval_size: usize,
}

/// seed for a specific PRG
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct AESSeed {
    bytes: Bytes,
}

/// evaluation type for seed-homomorphic PRG
impl AESSeed {
    pub fn to_field_element(&self, field: Field) -> FieldElement {
        field.element_from_bytes(&self.bytes)
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
    pub fn new(seed_size: usize, eval_size: usize) -> Self {
        assert!(
            seed_size <= eval_size,
            "eval size must be at least the seed size"
        );

        AESPRG {
            seed_size,
            eval_size,
        }
    }
}

// Implementation of an AES-based PRG
impl PRG for AESPRG {
    type Seed = AESSeed;
    type Output = Bytes;

    /// generates a new (random) seed for the given PRG
    fn new_seed(&self) -> AESSeed {
        let mut seed_bytes = vec![0; self.seed_size];
        thread_rng().fill_bytes(&mut seed_bytes);

        AESSeed {
            bytes: Bytes::from(seed_bytes),
        }
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
        let cipher = Cipher::aes_128_ctr();
        let mut ciphertext = encrypt(
            cipher,
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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashSet;
    use std::fmt::Debug;
    use std::ops::Range;

    const SIZES: Range<usize> = 16..1000;
    const SEED_SIZE: usize = 16;

    impl Arbitrary for AESPRG {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            SIZES
                .prop_map(|output_size| AESPRG::new(SEED_SIZE, output_size))
                .boxed()
        }
    }

    impl Arbitrary for AESSeed {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            prop::collection::vec(any::<u8>(), SEED_SIZE)
                .prop_map(|data| AESSeed { bytes: data.into() })
                .boxed()
        }
    }

    fn run_test_prg_seed_random<P>(prg: P)
    where
        P: PRG,
        P::Seed: Eq + Debug,
        P::Output: Eq + Debug,
    {
        assert_ne!(prg.new_seed(), prg.new_seed());
    }

    fn run_test_prg_eval_deterministic<P>(prg: P, seed: P::Seed)
    where
        P: PRG,
        P::Seed: Eq + Debug,
        P::Output: Eq + Debug,
    {
        assert_eq!(prg.eval(&seed), prg.eval(&seed));
    }

    fn run_test_prg_eval_random<P>(prg: P, seeds: &[P::Seed])
    where
        P: PRG,
        P::Seed: Eq + Debug,
        P::Output: Eq + Debug + Hash,
    {
        #![allow(clippy::mutable_key_type)] // https://github.com/rust-lang/rust-clippy/issues/5043
        let results: HashSet<_> = seeds.iter().map(|s| prg.eval(s)).collect();
        assert_eq!(results.len(), seeds.len());
    }

    proptest! {
        #[test]
        fn test_aes_prg_seed_random(prg in any::<AESPRG>()) {
            run_test_prg_seed_random(prg);
        }

        #[test]
        fn test_aes_prg_eval_deterministic(
            prg in any::<AESPRG>(),
            seed in any::<AESSeed>()
        ) {
            run_test_prg_eval_deterministic(prg, seed);
        }

        #[test]
        fn test_aes_prg_eval_random(
            prg in any::<AESPRG>(),
            seeds in prop::collection::vec(any::<AESSeed>(), 10),
        ) {
            run_test_prg_eval_random(prg, &seeds);
        }
    }
}
