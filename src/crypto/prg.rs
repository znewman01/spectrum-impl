//! Spectrum implementation.
use crate::crypto::field::{Field, FieldElement};
use bytes::Bytes;
use openssl::symm::{encrypt, Cipher};
use rand::prelude::*;
use std::rc::Rc;

const SEED_SIZE: usize = 16; // 16 bytes for AES 128

/// PRG uses AES to expand a seed to desired length
#[derive(Default, Clone, PartialEq, Debug, Copy)]
pub struct PRG {}

/// seed for a specific PRG
#[derive(Clone, PartialEq, Debug)]
pub struct PRGSeed {
    bytes: Bytes,
}

impl PRG {
    /// generate new PRG: seed_size -> eval_size
    pub fn new() -> PRG {
        PRG {}
    }

    /// generates a new (random) seed for the given PRG
    pub fn new_seed(self) -> PRGSeed {
        // seed is just random bytes
        let mut key = vec![0; SEED_SIZE];
        thread_rng().fill_bytes(&mut key);

        PRGSeed {
            bytes: Bytes::from(key),
        }
    }

    /// evaluates the PRG on the given seed
    pub fn eval(self, seed: &PRGSeed, eval_size: usize) -> Bytes {
        assert!(
            SEED_SIZE <= eval_size,
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
            &seed.bytes, // use seed bytes as the AES "key"
            Some(&iv),
            &data,
        )
        .unwrap();

        // truncate to correct expanded size
        ciphertext.truncate(eval_size);
        ciphertext.into()
    }
}

impl PRGSeed {
    pub fn to_field_element(&self, field: Rc<Field>) -> FieldElement {
        FieldElement::from_bytes(&self.bytes, field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prg_seed() {
        let prg = PRG::new();

        // PRG seed is of the correct size
        let seed = prg.new_seed();
        assert_eq!(seed.bytes.len(), SEED_SIZE);

        // PRG seed is random
        let seed = prg.new_seed();
        assert_ne!(seed.bytes, vec![0; SEED_SIZE]);
    }

    #[test]
    fn test_prg_eval() {
        let eval_size: usize = 1 << 16;
        let prg = PRG::new();

        // PRG output is non-zero
        let seed = prg.new_seed();
        let eval_bytes = prg.eval(&seed, eval_size);
        let all_zero = vec![0; eval_size];
        assert_ne!(eval_bytes, all_zero);

        // PRG output is of the correct size
        assert_eq!(eval_bytes.len(), eval_size);

        let prg = PRG::new();
        let seed = prg.new_seed();
        let eval1 = prg.eval(&seed, eval_size);
        let eval2 = prg.eval(&seed, eval_size);

        // PRG eval on the same seed should give the same output
        assert_eq!(eval1, eval2);

        let prg = PRG::new();
        let seed_prime = prg.new_seed();
        let eval1 = prg.eval(&seed, eval_size);
        let eval2 = prg.eval(&seed_prime, eval_size);

        // PRG eval on the diff seeds should give the diff output
        assert_ne!(eval1, eval2);
    }
}
