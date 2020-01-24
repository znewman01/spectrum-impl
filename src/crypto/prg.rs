//! Spectrum implementation.
extern crate crypto;

use crate::crypto::msg::Message;
use crypto::buffer::{BufferResult, ReadBuffer, WriteBuffer};
use crypto::{aes, blockmodes, buffer, symmetriccipher};
use rug::{rand::RandState, Assign, Integer};
use std::rc::Rc;
use rand::prelude::*;

/// PRG uses AES to expand a seed to desired length
#[derive(Clone, PartialEq, Debug)]
pub struct PRG {
    seed_size: usize,
    eval_size: usize,
}

/// seed for a specific PRG
#[derive(Clone, PartialEq, Debug)]
pub struct PRGSeed {
    bytes: Vec<u8>,
}

impl PRG {
    /// generate new PRG: seed_size -> eval_size
    pub fn new(seed_size: usize, eval_size: usize) -> PRG {
        PRG {
            seed_size: seed_size,
            eval_size: eval_size,
        }
    }

    /// generates a new (random) seed for the given PRG
    pub fn new_seed(&self) -> PRGSeed {
        // seed is just random bytes
        let mut key = vec![0; self.seed_size];
        thread_rng().fill_bytes(&mut key);        

        PRGSeed { bytes: key }
    }

    /// evaluates the PRG on the given seed
    pub fn eval(&self, seed: &PRGSeed) -> Message {
        // nonce set to zero: PRG eval should be deterministic
        let iv: [u8; 16] = [0; 16];

        // code below yanked from https://github.com/DaGenix/rust-crypto/
        // basically does an AES encryption of the all-zero string
        // with the PRG seed as the key
        let mut encryptor = aes::cbc_encryptor(
            aes::KeySize::KeySize128,
            &seed.bytes,
            &iv,
            blockmodes::PkcsPadding,
        );

        // 4096, default buffer size suggested in
        // https://github.com/DaGenix/rust-crypto/
        let mut buffer = [0; 4096];

        // data is what AES will be "encrypting", just here for size.
        let data = vec![0; self.eval_size];
        let mut write_buffer = buffer::RefWriteBuffer::new(&mut buffer);
        let mut read_buffer = buffer::RefReadBuffer::new(&data);

        // final_result will contain the final random bytes corresponding
        // to the PRG expansion / evaluation
        let mut final_result = Vec::<u8>::new();

        // encrypt the data in blocks of size 4096, exit when all blocks are processed
        loop {
            let result = encryptor.encrypt(&mut read_buffer, &mut write_buffer, true);
            final_result.extend(
                write_buffer
                    .take_read_buffer()
                    .take_remaining()
                    .iter()
                    .map(|&i| i),
            );

            match result.unwrap() {
                BufferResult::BufferUnderflow => break,
                BufferResult::BufferOverflow => {}
            }
        }

        assert!(self.eval_size <= final_result.len());

        // cut off extra bits generated by expansion until, this results in
        // the value of the expanded PRG
        final_result.truncate(self.eval_size);

        Message { data: final_result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prg_seed() {
        let seed_size: usize = 16;
        let eval_size: usize = 1 << 16;
        let prg = PRG::new(seed_size, eval_size);

        // PRG seed is of the correct size
        let seed = prg.new_seed();
        assert_eq!(seed.bytes.len(), seed_size);

        // PRG seed is random
        let seed = prg.new_seed();
        assert_ne!(seed.bytes, vec![0; seed_size]);
    }

    #[test]
    fn test_prg_eval() {
        let seed_size: usize = 16;
        let eval_size: usize = 1 << 16;
        let prg = PRG::new(seed_size, eval_size);

        // PRG output is non-zero
        let seed = prg.new_seed();
        let eval_msg = prg.eval(&seed);
        let all_zero = vec![0; eval_size];
        assert_ne!(eval_msg.data, all_zero);

        // PRG output is of the correct size
        assert_eq!(eval_msg.data.len(), eval_size);

        let prg = PRG::new(seed_size, eval_size);
        let seed = prg.new_seed();
        let eval1 = prg.eval(&seed);
        let eval2 = prg.eval(&seed);

        // PRG eval on the same seed should give the same output
        assert_eq!(eval1, eval2);

        let prg = PRG::new(seed_size, eval_size);
        let seed_prime = prg.new_seed();
        let eval1 = prg.eval(&seed);
        let eval2 = prg.eval(&seed_prime);

        // PRG eval on the diff seeds should give the diff output
        assert_ne!(eval1, eval2);
    }
}
