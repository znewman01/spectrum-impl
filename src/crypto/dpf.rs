//! Spectrum implementation.
extern crate rand;
use crate::crypto::byte_utils::xor_bytes;
use crate::crypto::prg::{PRGSeed, PRG};
use bytes::Bytes;
use rand::Rng;
use std::fmt::Debug;
use std::rc::Rc;

/// Distributed Point Function
/// Must generate a set of keys k_1, k_2, ...
/// such that combine(eval(k_1), eval(k_2), ...) = e_i * msg
pub trait DPF<Key> {
    fn new(security_bytes: usize, num_keys: usize, num_points: usize) -> Self;
    /// Generate `num_keys` DPF keys, the results of which differ only at the given index.
    fn gen(&self, msg: Bytes, idx: usize) -> Vec<Key>;
    fn eval(&self, key: &Key) -> Vec<Bytes>;
    fn compressed_eval(&self, key: &Key, tokens: &[Bytes]) -> Bytes;
    fn combine(&self, parts: Vec<Vec<Bytes>>) -> Vec<Bytes>;
}

/// DPF based on PRG
#[derive(Clone, PartialEq, Debug)]
pub struct PRGBasedDPF {
    security_bytes: usize,
    num_keys: usize,
    num_points: usize,
}

// DPF key for PRGBasedDPF
#[derive(Clone, PartialEq, Debug)]
pub struct DPFKey {
    prg: Rc<PRG>,
    pub encoded_msg: Bytes,
    pub bits: Vec<u8>,
    pub seeds: Vec<PRGSeed>,
}

impl DPF<DPFKey> for PRGBasedDPF {
    fn new(security_bytes: usize, num_keys: usize, num_points: usize) -> PRGBasedDPF {
        PRGBasedDPF {
            security_bytes,
            num_keys,
            num_points,
        }
    }

    /// generate new instance of PRG based DPF with two DPF keys
    fn gen(&self, msg: Bytes, idx: usize) -> Vec<DPFKey> {
        if self.num_keys != 2 {
            panic!("not implemented!")
        }

        // make a new PRG going from security -> length of the Bytes
        let prg = Rc::new(PRG::new(self.security_bytes, msg.len()));

        let mut seeds_a: Vec<PRGSeed> = Vec::new();
        let mut seeds_b: Vec<PRGSeed> = Vec::new();
        let mut bits_a: Vec<u8> = Vec::new();
        let mut bits_b: Vec<u8> = Vec::new();

        // generate the values distributed to servers A and B
        for j in 0..self.num_points {
            let seed = prg.new_seed();
            let bit = rand::thread_rng().gen_range(0, 2);

            seeds_a.push(seed.clone());
            bits_a.push(bit);

            if j == idx {
                let seed_prime = prg.new_seed();
                seeds_b.push(seed_prime);
                bits_b.push(1 - bit);
            } else {
                seeds_b.push(seed.clone());
                bits_b.push(bit);
            }
        }

        // compute G(seed_a) XOR G(seed_b) for the ith seed
        let xor_eval = xor_bytes(&prg.eval(&seeds_a[idx]), &prg.eval(&seeds_b[idx]));

        // compute m XOR G(seed_a) XOR G(seed_b)
        let encoded_msg = xor_bytes(&msg, &xor_eval);

        let mut key_tuple = Vec::<DPFKey>::new();
        key_tuple.push(DPFKey::new(
            prg.clone(),
            encoded_msg.clone(),
            bits_a,
            seeds_a,
        ));
        key_tuple.push(DPFKey::new(prg, encoded_msg, bits_b, seeds_b));

        key_tuple
    }

    /// evaluates the DPF on a given DPFKey and outputs the resulting data
    fn eval(&self, key: &DPFKey) -> Vec<Bytes> {
        key.seeds
            .iter()
            .zip(key.bits.iter())
            .map(|(seed, &bits)| {
                let prg_eval_i = key.prg.eval(seed);

                if bits == 1 {
                    xor_bytes(&key.encoded_msg.clone(), &prg_eval_i)
                } else {
                    prg_eval_i
                }
            })
            .collect()
    }

    /// evaluates the DPF on a given DPFKey to generate a set a point vector
    // TODO(sss): find a better / more representative name for this functionality
    fn compressed_eval(&self, key: &DPFKey, tokens: &[Bytes]) -> Bytes {
        assert_eq!(key.bits.len(), tokens.len());

        let mut res = Bytes::from(vec![0; key.prg.seed_size]);

        for (i, (seed, bit)) in key.seeds.iter().zip(key.bits.iter()).enumerate() {
            if *bit != 0 {
                res = xor_bytes(&res, seed.raw_bytes());
                res = xor_bytes(&res, &tokens[i]);
            }
        }

        res
    }

    /// combines the results produced by running eval on both keys
    fn combine(&self, parts: Vec<Vec<Bytes>>) -> Vec<Bytes> {
        // xor all the parts together
        let mut res = parts[0].clone();
        for part in parts.iter().skip(1) {
            for j in 0..res.len() {
                res[j] = xor_bytes(&res[j], &part[j]);
            }
        }

        res
    }
}

impl DPFKey {
    // generates a new DPF key with the necessary parameters needed for evaluation
    pub fn new(prg: Rc<PRG>, encoded_msg: Bytes, bits: Vec<u8>, seeds: Vec<PRGSeed>) -> DPFKey {
        DPFKey {
            prg,
            encoded_msg,
            bits,
            seeds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const DATA_SIZE: usize = (1 << 10);
    const MAX_NUM_POINTS: usize = 20;
    const SECURITY_BYTES: usize = 16;

    fn num_keys() -> impl Strategy<Value = usize> {
        Just(2)
    }

    fn num_points_and_index() -> impl Strategy<Value = (usize, usize)> {
        (1..MAX_NUM_POINTS).prop_flat_map(|num_points| (Just(num_points), 0..num_points))
    }

    fn data() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(any::<u8>(), DATA_SIZE)
    }

    proptest! {
        #[test]
        fn test_prg_dpf(
            (num_points, index) in num_points_and_index(),
            num_keys in num_keys(),
            data in data()
        ) {
            let dpf = PRGBasedDPF::new(SECURITY_BYTES, num_keys, num_points);
            let actual = dpf.combine(
                dpf.gen(Bytes::from(data.clone()), index)
                    .iter()
                    .map(|key| dpf.eval(key))
                    .collect()
            );
            let zeroes = vec![0 as u8; DATA_SIZE];
            let mut expected = vec![zeroes; num_points - 1];
            expected.insert(index, data);
            assert_eq!(actual, expected);
        }
    }
}
