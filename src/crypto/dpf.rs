//! Spectrum implementation.
#![allow(dead_code)]
use crate::bytes::Bytes;
use crate::crypto::prg::PRG;
use derivative::Derivative;
use rand::{thread_rng, Rng};
use std::fmt::Debug;
use std::iter::repeat_with;

/// Distributed Point Function
/// Must generate a set of keys k_1, k_2, ...
/// such that combine(eval(k_1), eval(k_2), ...) = e_i * msg
pub trait DPF {
    type Key;

    fn num_points(&self) -> usize;
    fn num_keys(&self) -> usize;

    /// Generate `num_keys` DPF keys, the results of which differ only at the given index.
    fn gen(&self, msg: &Bytes, idx: usize) -> Vec<Self::Key>;
    fn gen_empty(&self, len: usize) -> Vec<Self::Key>;
    fn eval(&self, key: &Self::Key) -> Vec<Bytes>;
    fn combine(&self, parts: Vec<Vec<Bytes>>) -> Vec<Bytes>;
}

/// DPF based on PRG
#[derive(Clone, PartialEq, Debug)]
pub struct PRGDPF<P> {
    prg: P,
    num_keys: usize,
    num_points: usize,
}

// DPF key for PRGDPF
#[derive(Derivative)]
#[derivative(
    Debug(bound = "P::Seed: Debug"),
    PartialEq(bound = "P::Seed: PartialEq"),
    Eq(bound = "P::Seed: Eq"),
    Clone(bound = "P::Seed: Clone")
)]
pub struct PRGKey<P: PRG> {
    pub encoded_msg: Bytes,
    pub bits: Vec<u8>,
    pub seeds: Vec<<P as PRG>::Seed>,
}

impl<P: PRG> PRGKey<P> {
    // generates a new DPF key with the necessary parameters needed for evaluation
    pub fn new(encoded_msg: Bytes, bits: Vec<u8>, seeds: Vec<P::Seed>) -> PRGKey<P> {
        assert_eq!(bits.len(), seeds.len());
        assert!(
            bits.iter().all(|b| *b == 0 || *b == 1),
            "All bits must be 0 or 1"
        );
        PRGKey {
            encoded_msg,
            bits,
            seeds,
        }
    }
}

impl<P> PRGDPF<P> {
    pub fn new(prg: P, num_keys: usize, num_points: usize) -> PRGDPF<P> {
        assert_eq!(num_keys, 2, "DPF only implemented for s=2.");
        PRGDPF {
            prg,
            num_keys,
            num_points,
        }
    }
}

impl<P> DPF for PRGDPF<P>
where
    P: PRG + Clone,
    P::Seed: Clone + PartialEq + Eq + Debug,
{
    type Key = PRGKey<P>;

    fn num_points(&self) -> usize {
        self.num_points
    }

    fn num_keys(&self) -> usize {
        self.num_keys
    }

    /// generate new instance of PRG based DPF with two DPF keys
    fn gen(&self, msg: &Bytes, point_idx: usize) -> Vec<PRGKey<P>> {
        let seeds_a: Vec<_> = repeat_with(|| self.prg.new_seed())
            .take(self.num_points)
            .collect();
        let bits_a: Vec<_> = repeat_with(|| thread_rng().gen_range(0, 2))
            .take(self.num_points)
            .collect();

        let mut seeds_b = seeds_a.clone();
        seeds_b[point_idx] = self.prg.new_seed();

        let mut bits_b = bits_a.clone();
        bits_b[point_idx] = 1 - bits_b[point_idx];

        let encoded_msg = self.prg.eval(&seeds_a[point_idx], msg.len())
            ^ &self.prg.eval(&seeds_b[point_idx], msg.len())
            ^ msg;

        vec![
            PRGKey::new(encoded_msg.clone(), bits_a, seeds_a),
            PRGKey::new(encoded_msg, bits_b, seeds_b),
        ]
    }

    fn gen_empty(&self, size: usize) -> Vec<PRGKey<P>> {
        let seeds: Vec<_> = repeat_with(|| self.prg.new_seed())
            .take(self.num_points)
            .collect();
        let bits: Vec<_> = repeat_with(|| thread_rng().gen_range(0, 2))
            .take(self.num_points)
            .collect();
        let encoded_msg = Bytes::random(size, &mut thread_rng());

        vec![PRGKey::new(encoded_msg, bits, seeds); 2]
    }

    /// evaluates the DPF on a given PRGKey and outputs the resulting data
    fn eval(&self, key: &PRGKey<P>) -> Vec<Bytes> {
        key.seeds
            .iter()
            .zip(key.bits.iter())
            .map(|(seed, bits)| {
                let mut data = self.prg.eval(seed, key.encoded_msg.len());
                if *bits == 1 {
                    data ^= &key.encoded_msg
                }
                data
            })
            .collect()
    }

    /// combines the results produced by running eval on both keys
    fn combine(&self, parts: Vec<Vec<Bytes>>) -> Vec<Bytes> {
        let mut parts = parts.iter();
        let mut res = parts
            .next()
            .expect("Need at least one part to combine.")
            .clone();
        for part in parts {
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

    const DATA_SIZE: usize = 20;
    const MAX_NUM_POINTS: usize = 10;

    impl<P: Arbitrary + 'static> Arbitrary for PRGDPF<P> {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            let num_keys = 2; // PRG DPF implementation handles only 2 keys.
            (any::<P>(), 1..=MAX_NUM_POINTS)
                .prop_map(move |(prg, num_points)| PRGDPF::new(prg, num_keys, num_points))
                .boxed()
        }
    }

    impl<P> Arbitrary for PRGKey<P>
    where
        P: PRG,
        P::Seed: Arbitrary,
    {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            (1..10usize)
                .prop_flat_map(|num_keys| {
                    (
                        any::<Bytes>(),
                        prop::collection::vec(0..1u8, num_keys),
                        prop::collection::vec(any::<P::Seed>(), num_keys),
                    )
                        .prop_map(|(msg, bits, seeds)| Self::new(msg, bits, seeds))
                })
                .boxed()
        }
    }

    fn data() -> impl Strategy<Value = Bytes> {
        prop::collection::vec(any::<u8>(), DATA_SIZE).prop_map(Bytes::from)
    }

    fn run_test_dpf<D: DPF>(dpf: D, data: Bytes, index: usize) {
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

    fn run_test_dpf_empty<D: DPF>(dpf: D, size: usize) {
        let dpf_keys = dpf.gen_empty(size);
        let dpf_shares = dpf_keys.iter().map(|k| dpf.eval(k)).collect();
        let dpf_output = dpf.combine(dpf_shares);

        for chunk in dpf_output {
            let zeroes = Bytes::empty(DATA_SIZE);
            assert_eq!(chunk, zeroes);
        }
    }

    proptest! {
        #[test]
        fn test_prg_dpf(
            dpf in any::<PRGDPF<AESPRG>>(),
            index in any::<proptest::sample::Index>(),
            data in data()
        ) {
            let index = index.index(dpf.num_points());
            run_test_dpf(dpf, data, index);
        }

        #[test]
        fn test_prg_dpf_empty(
            dpf in any::<PRGDPF<AESPRG>>(),
            size in Just(DATA_SIZE),
        ) {
            run_test_dpf_empty(dpf, size);
        }
    }
}
