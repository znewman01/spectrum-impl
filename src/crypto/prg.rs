//! Spectrum implementation.
extern crate crypto;
extern crate rand;
use crate::crypto::group::Group;
use crypto::buffer::{BufferResult, ReadBuffer, WriteBuffer};
use crypto::{aes, blockmodes, buffer, symmetriccipher};
use rand::{OsRng, Rng};
use rug::{rand::RandState, Assign, Integer};
use std::rc::Rc;

// vanilla and seed-homomorphic PRG
// vanilla PRG uses AES to expand a seed to desired length
// seed-homomorphic PRG uses group operations in a cyclic group
// to expand a seed to a desired length
// see https://crypto.stanford.edu/~dabo/pubs/papers/homprf.pdf
// for details on seed-homomorphic PRGs
struct PRG {
    seed_size: usize,
    eval_size: usize,
    is_seed_hom: bool,
    gp: Option<std::rc::Rc<Group>>, // group for seed-hom prg
    gens: Option<Vec<Integer>>,     // generators for group gp, used in seed-hom prg expansion
}

// seed for a PRG (either vanilla or seed-homomorphic)
struct PRGSeed {
    bytes: Vec<u8>,
    prg: std::rc::Rc<PRG>,
}

impl PRG {
    // generate new PRG: seed_size -> eval_size
    // where is_seed_hom true will make the PRG seed homomorphic
    pub fn new(seed_size: usize, eval_size: usize, is_seed_hom: bool) -> PRG {
        if !is_seed_hom {
            return PRG {
                seed_size: seed_size,
                eval_size: eval_size,
                is_seed_hom: false,
                gp: None,
                gens: None,
            };
        } else {
            // generate random number with seed_size bits
            // recall: seed_size in *bytes* so mult by 8
            let mut rand = RandState::new();
            let num_bits: u32 = seed_size as u32;
            let rand_bits = Integer::from(Integer::random_bits(num_bits, &mut rand));

            // get prime of seed_size*8 bits
            let prime = rand_bits.next_prime();
            let num_gen_needed = eval_size / seed_size;

            // generate the necessary number of generators for
            // the group to expand from seed_size to eval_size
            let mut gens: Vec<Integer> = vec![Integer::new(); num_gen_needed];
            for i in 0..num_gen_needed {
                gens[i] = prime.clone().random_below(&mut rand);
            }

            // gp has generator g = 0; only gens are used in seed-homomorphic PRG implementation
            let gp = std::rc::Rc::<Group>::new(Group::new(Integer::new(), prime));

            return PRG {
                seed_size: seed_size,
                eval_size: eval_size,
                is_seed_hom: true,
                gp: Some(gp),
                gens: Some(gens),
            };
        }
    }
}

impl PRGSeed {
    // generate new (random) seed for the given PRG
    pub fn new(prg: std::rc::Rc<PRG>) -> PRGSeed {
        if !prg.is_seed_hom {
            // in vanilla PRG, seed is just random bytes
            let mut key = vec![0; prg.seed_size];
            let mut rng = OsRng::new().ok().unwrap();
            rng.fill_bytes(&mut key);

            return PRGSeed {
                bytes: key,
                prg: prg,
            };
        } else {
            // in seed-hom implementation the seed is a random group element
            // which gets stored as bytes via a string
            let mut rand = RandState::new();
            let bound = prg.gp.as_ref().unwrap().p.clone();
            let seed = bound.random_below(&mut rand);

            return PRGSeed {
                bytes: seed.to_string_radix(16).into_bytes(),
                prg: prg,
            };
        }
    }

    pub fn hommomorphic_eval(self) -> Vec<u8> {
        // TODO: [for=sss] figure out and implement.
        let stub: Vec<u8> = vec![0; 0];
        return stub;
    }

    pub fn eval(self) -> Vec<u8> {
        if !self.prg.is_seed_hom {
            // nonce set to zero: PRG eval should be deterministic
            let iv: [u8; 16] = [0; 16];
            let data = vec![0; self.prg.eval_size];

            // code below yanked from https://github.com/DaGenix/rust-crypto/
            // basically does an AES encryption of the all-zero string
            // with the PRG seed as the key
            let mut encryptor = aes::cbc_encryptor(
                aes::KeySize::KeySize256,
                &self.bytes,
                &iv,
                blockmodes::PkcsPadding,
            );
            let mut final_result = Vec::<u8>::new();
            let mut read_buffer = buffer::RefReadBuffer::new(&data);
            let mut buffer = [0; 4096];
            let mut write_buffer = buffer::RefWriteBuffer::new(&mut buffer);
            encryptor.encrypt(&mut read_buffer, &mut write_buffer, true);
            final_result.extend(
                write_buffer
                    .take_read_buffer()
                    .take_remaining()
                    .iter()
                    .map(|&i| i),
            );

            return final_result;
        } else {
            // for the seed-homomorphic version first convert the seed bytes
            // back to an Integer (via a string) equal to the seed value s and then evaluate
            // each generator g_1^s, g_2^2 ... g_n^s which gets
            // concatenated together and converted to bytes

            // TODO: [for=sss] figure out if this actually works. There might be issues with this approach. Just a hack for now.

            let mut res_string: String = "".to_owned();
            let seed_str = std::str::from_utf8(&self.bytes).unwrap();
            let seed = Integer::from_str_radix(&seed_str, 16).unwrap();

            let num_gens = self.prg.gens.as_ref().unwrap().len();
            for i in 0..num_gens {
                let gen_i = &self.prg.gens.as_ref().unwrap()[i].clone();
                let val = match gen_i
                    .clone()
                    .pow_mod(&seed, &self.prg.gp.as_ref().unwrap().p)
                {
                    Ok(power) => power,
                    Err(_) => unreachable!(),
                };

                res_string.push_str(&val.to_string_radix(16));
            }

            return res_string.into_bytes();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prg() {
        // TODO: [for=sss] design a better test. Now only checking that PRG doesn't output all zero string...
        let seed_size: usize = 16;
        let eval_size: usize = 1 << 16;

        let prg = std::rc::Rc::<PRG>::new(PRG::new(seed_size, eval_size, false));
        let seed = PRGSeed::new(prg);
        let eval = seed.eval();
        let all_zero = vec![0; seed_size];
        assert_ne!(eval, all_zero);
    }

    #[test]
    fn test_seed_homomorphic_prg() {
        // TODO: [for=sss] design a better test. Need to evaluate homomorphic property
        let seed_size: usize = 16;
        let eval_size: usize = 1 << 16;

        let prg = std::rc::Rc::<PRG>::new(PRG::new(seed_size, eval_size, true));
        let seed = PRGSeed::new(prg);
        let eval = seed.eval();
        let all_zero = vec![0; seed_size];
        assert_ne!(eval, all_zero);
    }
}
