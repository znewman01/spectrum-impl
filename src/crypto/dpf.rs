//! Spectrum implementation.
#![allow(dead_code)]
use crate::crypto::byte_utils::Bytes;
use crate::crypto::prg::PRG;
use rand::Rng;
use std::fmt::Debug;

/// Distributed Point Function
/// Must generate a set of keys k_1, k_2, ...
/// such that combine(eval(k_1), eval(k_2), ...) = e_i * msg
pub trait DPF {
    type Key;

    fn num_points(&self) -> usize;
    fn num_keys(&self) -> usize;

    /// Generate `num_keys` DPF keys, the results of which differ only at the given index.
    fn gen(&self, msg: &Bytes, idx: usize) -> Vec<Self::Key>;
    fn eval(&self, key: &Self::Key) -> Vec<Bytes>;
    fn combine(&self, parts: Vec<Vec<Bytes>>) -> Vec<Bytes>;
}

/// DPF based on PRG
#[derive(Clone, PartialEq, Debug)]
pub struct PRGBasedDPF<P> {
    prg: P,
    num_keys: usize,
    num_points: usize,
}

// DPF key for PRGBasedDPF
#[derive(Clone, PartialEq, Debug)]
pub struct DPFKey<P>
where
    P: PRG,
    P::Seed: Clone + PartialEq + Eq + Debug,
{
    pub encoded_msg: Bytes,
    pub bits: Vec<u8>,
    pub seeds: Vec<<P as PRG>::Seed>,
}

impl<P> DPFKey<P>
where
    P: PRG,
    P::Seed: Clone + PartialEq + Eq + Debug,
{
    // generates a new DPF key with the necessary parameters needed for evaluation
    fn new(encoded_msg: Bytes, bits: Vec<u8>, seeds: Vec<P::Seed>) -> DPFKey<P> {
        DPFKey {
            encoded_msg,
            bits,
            seeds,
        }
    }
}

impl<P> PRGBasedDPF<P> {
    pub fn new(prg: P, num_keys: usize, num_points: usize) -> PRGBasedDPF<P> {
        PRGBasedDPF {
            prg,
            num_keys,
            num_points,
        }
    }
}

impl<P> DPF for PRGBasedDPF<P>
where
    P: PRG,
    P::Seed: Clone + PartialEq + Eq + Debug,
{
    type Key = DPFKey<P>;

    fn num_points(&self) -> usize {
        self.num_points
    }

    fn num_keys(&self) -> usize {
        self.num_keys
    }

    /// generate new instance of PRG based DPF with two DPF keys
    fn gen(&self, msg: &Bytes, idx: usize) -> Vec<DPFKey<P>> {
        assert_eq!(self.num_keys, 2, "DPF only implemented for s=2.");

        let mut seeds_a = vec![];
        let mut seeds_b = vec![];
        let mut bits_a: Vec<u8> = vec![];
        let mut bits_b: Vec<u8> = vec![];

        // generate the values distributed to servers A and B
        for j in 0..self.num_points {
            let seed = self.prg.new_seed();
            let bit = rand::thread_rng().gen_range(0, 2);

            seeds_a.push(seed.clone());
            bits_a.push(bit);

            if j == idx {
                seeds_b.push(self.prg.new_seed());
                bits_b.push(1 - bit);
            } else {
                seeds_b.push(seed.clone());
                bits_b.push(bit);
            }
        }

        let encoded_msg = self.prg.eval(&seeds_a[idx], msg.len())
            ^ &self.prg.eval(&seeds_b[idx], msg.len())
            ^ msg;

        vec![
            DPFKey::<P>::new(encoded_msg.clone(), bits_a, seeds_a),
            DPFKey::<P>::new(encoded_msg, bits_b, seeds_b),
        ]
    }

    /// evaluates the DPF on a given DPFKey and outputs the resulting data
    fn eval(&self, key: &DPFKey<P>) -> Vec<Bytes> {
        key.seeds
            .iter()
            .zip(key.bits.iter())
            .map(|(seed, &bits)| {
                let mask = self.prg.eval(seed, key.encoded_msg.len());

                if bits == 1 {
                    mask ^ &key.encoded_msg
                } else {
                    mask
                }
            })
            .collect()
    }

    /// combines the results produced by running eval on both keys
    fn combine(&self, parts: Vec<Vec<Bytes>>) -> Vec<Bytes> {
        // xor all the parts together
        let mut res = parts[0].clone();
        for part in parts.iter().skip(1) {
            for (x, y) in res.iter_mut().zip(part.iter()) {
                *x ^= y;
            }
        }

        res
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::crypto::prg::AESPRG;
    use proptest::prelude::*;

    const DATA_SIZE: usize = 1 << 10;
    const MAX_NUM_POINTS: usize = 20;

    pub fn aes_prg_dpfs() -> impl Strategy<Value = PRGBasedDPF<AESPRG>> {
        let prg = AESPRG::new();
        let num_keys = 2; // PRG DPF implementation handles only 2 keys.
        (1..MAX_NUM_POINTS).prop_map(move |num_points| PRGBasedDPF::new(prg, num_keys, num_points))
    }

    fn data() -> impl Strategy<Value = Bytes> {
        prop::collection::vec(any::<u8>(), DATA_SIZE).prop_map(Bytes::from)
    }

    fn run_test_dpf<D>(dpf: D, data: Bytes, index: usize)
    where
        D: DPF,
    {
        let dpf_keys = dpf.gen(&data, index);
        let dpf_shares = dpf_keys.iter().map(|k| dpf.eval(k)).collect();
        let dpf_output = dpf.combine(dpf_shares);

        for (chunk_idx, chunk) in dpf_output.into_iter().enumerate() {
            if chunk_idx == index {
                assert_eq!(chunk, data);
            } else {
                let zeroes = Bytes::empty(DATA_SIZE);
                assert_eq!(chunk, zeroes);
            }
        }
    }

    proptest! {
        #[test]
        fn test_prg_dpf(
            dpf in aes_prg_dpfs(),
            index in any::<proptest::sample::Index>(),
            data in data()
        ) {
            let index = index.index(dpf.num_points());
            run_test_dpf(dpf, data, index);
        }
    }
}
