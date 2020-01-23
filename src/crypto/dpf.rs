//! Spectrum implementation.
extern crate rand;
use crate::crypto::msg::Message;
use crate::crypto::prg::{PRGSeed, PRG};
use rand::Rng;
use rug::{integer::IsPrime, rand::RandState, Integer};
use std::fmt::Debug;
use std::ops;
use std::rc::Rc;

#[derive(Clone, PartialEq, Debug)]
pub struct DPF {
    key_a: DPFKey,
    key_b: DPFKey,
}

#[derive(Clone, PartialEq, Debug)]
pub struct DPFKey {
    prg: Rc<PRG>,
    encoded_msg: Message,
    bits: Vec<u8>,
    seeds: Vec<PRGSeed>,
}

impl DPF {
    /// generate new field element
    pub fn new(security_bytes: usize, msg: Message, i: usize, n: usize) -> DPF {

        let eval_size = msg.data.len();

        // make a new PRG going from security -> length of the message
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

        let prg_eval_a = prg.eval(seeds_a[i].clone());
        let prg_eval_b = prg.eval(seeds_b[i].clone());

        // compute G(seed_a) XOR G(seed_b) for the ith seed
        let xor_prgs_eval = prg_eval_a ^ prg_eval_b;

        // compute m XOR G(seed_a) XOR G(seed_b)
        let encoded_msg = msg ^ xor_prgs_eval;

        let key_a = DPFKey::new(prg.clone(), encoded_msg.clone(), bits_a, seeds_a);
        let key_b = DPFKey::new(prg.clone(), encoded_msg.clone(), bits_b, seeds_b);

        DPF {
            key_a: key_a,
            key_b: key_b,
        }
    }
}

impl DPFKey {
    // generates a new field element; v mod field.p
    pub fn new(prg: Rc<PRG>, encoded_msg: Message, bits: Vec<u8>, seeds: Vec<PRGSeed>) -> DPFKey {
        DPFKey {
            prg: prg,
            encoded_msg: encoded_msg,
            bits: bits,
            seeds: seeds,
        }
    }

    pub fn eval(self) -> Vec<Message> {
        // total number of slots
        let n = self.bits.len();

        // vector of slot messages
        let mut messages: Vec<Message> = Vec::<Message>::new();

        for i in 0..n {
            let message_i = self.prg.eval(self.seeds[i].clone());

            if self.bits[i] == 1 {
                messages.push(self.encoded_msg.clone() ^ message_i);
            } else {
                messages.push(message_i);
            }
        }

        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dpf_gen() {
        let data_size = (1 << 8) * 4096;
        let data: Vec<u8> = vec![0; data_size];
        let index = 1;
        let n = 20;

        let msg = Message::new(data.clone());
        let dpf = DPF::new(16, msg, index, n);

        // check that dpf seeds and bits differ only at index
        for i in 0..n {
            if i != index {
                assert_eq!(dpf.key_a.seeds[i], dpf.key_b.seeds[i]);
                assert_eq!(dpf.key_a.bits[i], dpf.key_b.bits[i]);
            } else {
                assert_ne!(dpf.key_a.seeds[i], dpf.key_b.seeds[i]);
                assert_ne!(dpf.key_a.bits[i], dpf.key_b.bits[i]);
            }
        }
    }

    #[test]
    fn test_dpf_eval() {
        let data_size = (1 << 8) * 4096;
        let data: Vec<u8> = vec![0; data_size];
        let index = 1;
        let n = 20;

        let msg = Message::new(data.clone());
        let dpf = DPF::new(16, msg, index, n);

        // check that dpf evaluates correctly
        let eval_res_a = dpf.key_a.eval();
        let eval_res_b = dpf.key_b.eval();

        // used compare dpf eval for index \neq i
        let null: Vec<u8> = vec![0; data_size];
        for i in 0..n {
            let eval_res = eval_res_a[i].clone() ^ eval_res_b[i].clone();
            if i != index {
                assert_eq!(eval_res.data, null);
            } else {
                assert_eq!(eval_res.data, data);
            }
        }
    }
}
