//! Spectrum implementation.
extern crate rand;
use crate::crypto::prg::{PRGSeed, PRG};
use bytes::Bytes;
use rand::Rng;
use rug::{integer::IsPrime, rand::RandState, Integer};
use std::fmt::Debug;
use std::ops;
use std::rc::Rc;

/// Distributed Point Function
/// Must generate a set of keys k_1, k_2, ... 
/// such that combine(eval(k_1), eval(k_2), ...) = e_i * msg
trait DPF {
    fn gen(security_bytes: usize, msg: Bytes, i: usize, n: usize) -> Vec<DPFKey>;
    fn eval(key: &DPFKey) -> Vec<Bytes>;
    fn combine(parts: Vec<Vec<Bytes>>) -> Vec<Bytes>;
}

/// DPF based on PRG
struct PRGBasedDPF {}

impl DPF for PRGBasedDPF {

     /// generate new instance of PRG based DPF with two DPF keys
    fn gen(security_bytes: usize, msg: Bytes, i: usize, n: usize) -> Vec<DPFKey> {
        let eval_size = msg.len();

        // make a new PRG going from security -> length of the Bytes
        let prg = Rc::<PRG>::new(PRG::new(security_bytes, eval_size));

        let mut seeds_a: Vec<PRGSeed> = Vec::<PRGSeed>::new();
        let mut seeds_b: Vec<PRGSeed> = Vec::<PRGSeed>::new();
        let mut bits_a: Vec<u8> = Vec::<u8>::new();
        let mut bits_b: Vec<u8> = Vec::<u8>::new();

        // generate the values distributed to servers A and B
        for j in 0..n {
            let seed = prg.new_seed();
            let bit = rand::thread_rng().gen_range(0, 2);

            seeds_a.push(seed.clone());
            bits_a.push(bit);

            if j == i {
                let seed_prime = prg.new_seed();
                seeds_b.push(seed_prime);
                bits_b.push(1 - bit);
            } else {
                seeds_b.push(seed.clone());
                bits_b.push(bit);
            }
        }

        let prg_eval_a= prg.eval(&seeds_a[i]);
        let prg_eval_b = prg.eval(&seeds_b[i]);

        // compute G(seed_a) XOR G(seed_b) for the ith seed
        let xor_eval = xor_bytes(&prg_eval_a, &prg_eval_b);

        // compute m XOR G(seed_a) XOR G(seed_b)
        let encoded_msg = xor_bytes(&msg, &xor_eval);

        let mut key_tuple = Vec::<DPFKey>::new();
        key_tuple.push(DPFKey::new(prg.clone(), encoded_msg.clone(), bits_a, seeds_a));
        key_tuple.push(DPFKey::new(prg.clone(), encoded_msg.clone(), bits_b, seeds_b));

        key_tuple
    }

    /// evaluates the DPF on a given DPFKey and outputs the resulting data
    fn eval(key: &DPFKey) -> Vec<Bytes> {
        // total number of slots
        let n = key.bits.len();
    
        // vector of slot Bytess
        let mut res: Vec<Bytes> = Vec::<Bytes>::new();
    
        for i in 0..n {
            let prg_eval_i = key.prg.eval(&key.seeds[i]);
    
            if key.bits[i] == 1 {
                let slot = xor_bytes(&key.encoded_msg.clone(), &prg_eval_i);
                res.push(slot);
            } else {
                res.push(prg_eval_i);
            }
        }
    
        res
    }

    /// combines the results produced by running eval on both keys
    fn combine(parts: Vec<Vec<Bytes>>) -> Vec<Bytes> {
        // xor all the parts together
        let mut res = parts[0].clone();
        for i in 1..parts.len() {
            for j in 0..res.len() {
                res[j] = xor_bytes(&res[j], &parts[i][j]);
            }
        }

        res
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct DPFKey {
    prg: Rc<PRG>,
    encoded_msg: Bytes,
    bits: Vec<u8>,
    seeds: Vec<PRGSeed>,
}


impl DPFKey {
    // generates a new DPF key with the necessary parameters needed for evaluation
    pub fn new(prg: Rc<PRG>, encoded_msg: Bytes, bits: Vec<u8>, seeds: Vec<PRGSeed>) -> DPFKey {
        DPFKey {
            prg: prg,
            encoded_msg: encoded_msg,
            bits: bits,
            seeds: seeds,
        }
    }
}

/// xor bytes in place, a = a ^ b
// TODO: (Performance) xor inplace rather than copying 
fn xor_bytes(a: &Bytes, b: &Bytes) -> Bytes {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b.iter()).map(|(&a, &b)| a ^ b).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prg_dpf_gen() {
        let data_size = (1 << 8) * 4096;
        let data: Vec<u8> = vec![0; data_size];
        let index = 1;
        let n = 20;

        let msg = Bytes::from(data);
        let dpf_keys = PRGBasedDPF::gen(16, msg, index, n);

        // check that dpf seeds and bits differ only at index
        for i in 0..n {
            if i != index {
                assert_eq!(dpf_keys[0].seeds[i], dpf_keys[1].seeds[i]);
                assert_eq!(dpf_keys[0].bits[i], dpf_keys[1].bits[i]);
            } else {
                assert_ne!(dpf_keys[0].seeds[i], dpf_keys[1].seeds[i]);
                assert_ne!(dpf_keys[0].bits[i], dpf_keys[1].bits[i]);
            }
        }
    }

    #[test]
    fn test_prg_dpf_eval() {
        let data_size = (1 << 8) * 4096;
        let data: Vec<u8> = vec![0; data_size];
        let index = 1;
        let n = 20;

        let msg = Bytes::from(data.clone());
        let dpf_keys = PRGBasedDPF::gen(16, msg, index, n);

        // check that dpf evaluates correctly
        let eval_res_a = PRGBasedDPF::eval(&dpf_keys[0]);
        let eval_res_b = PRGBasedDPF::eval(&dpf_keys[1]);

        // used compare dpf eval for index \neq i
        let null: Vec<u8> = vec![0; data_size];
        for i in 0..n {
            let eval_res = xor_bytes(&eval_res_a[i], &eval_res_b[i]);
            if i != index {
                assert_eq!(eval_res, null);
            } else {
                assert_eq!(eval_res, data);
            }
        }
    }

    #[test]
    fn test_prg_dpf_combine() {
        let data_size = (1 << 8) * 4096;
        let data: Vec<u8> = vec![0; data_size];
        let index = 1;
        let n = 20;

        let msg = Bytes::from(data.clone());
        let dpf_keys = PRGBasedDPF::gen(16, msg, index, n);

        // check that dpf evaluates correctly
        let mut results = Vec::<Vec<Bytes>>::new();
        results.push(PRGBasedDPF::eval(&dpf_keys[0]));
        results.push(PRGBasedDPF::eval(&dpf_keys[1]));

        let eval_res = PRGBasedDPF::combine(results);
        let null: Vec<u8> = vec![0; data_size];

        for i in 0..n {
            if i != index {
                assert_eq!(eval_res[i], null);
            } else {
                assert_eq!(eval_res[i], data);
            }
        }
    }
}
