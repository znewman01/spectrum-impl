//! 2-DPF (i.e. num_keys = 2) based on any PRG G(.).
use std::fmt::Debug;
use std::iter::repeat_with;
use std::ops;
use std::sync::Arc;

use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};

use super::DPF;
use crate::prg::PRG;

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Construction<P> {
    prg: P,
    num_points: usize,
}

impl<P> Construction<P> {
    pub fn new(prg: P, num_points: usize) -> Construction<P> {
        Construction { prg, num_points }
    }
}

#[derive(Clone)]
pub struct Key<M, S> {
    pub encoded_msg: M, //P::Output,
    pub bits: Vec<u8>,
    pub seeds: Vec<S>, // Vec<<P as PRG>::Seed>,
}

impl<M, S> Key<M, S> {
    fn new(encoded_msg: M, bits: Vec<u8>, seeds: Vec<S>) -> Self {
        Key {
            encoded_msg,
            bits,
            seeds,
        }
    }
}

impl<P> DPF for Construction<P>
where
    P: PRG + Clone,
    P::Seed: Clone + PartialEq + Eq + Debug,
    P::Output: Clone
        + PartialEq
        + Eq
        + Debug
        + ops::BitXor<P::Output, Output = P::Output>
        + ops::BitXor<Arc<P::Output>, Output = P::Output>
        + ops::BitXorAssign<P::Output>,
{
    type Key = Key<P::Output, P::Seed>;
    type Message = P::Output;

    fn num_points(&self) -> usize {
        self.num_points
    }

    fn num_keys(&self) -> usize {
        2 // this construction only works for s = 2
    }

    fn msg_size(&self) -> usize {
        self.prg.output_size()
    }

    fn null_message(&self) -> Self::Message {
        self.prg.null_output()
    }

    /// generate new instance of PRG based DPF with two DPF keys
    fn gen(&self, msg: Self::Message, point_idx: usize) -> Vec<Self::Key> {
        let seeds_a: Vec<_> = repeat_with(|| self.prg.new_seed())
            .take(self.num_points)
            .collect();
        let bits_a: Vec<_> = repeat_with(|| thread_rng().gen_range(0..2))
            .take(self.num_points)
            .collect();

        let mut seeds_b = seeds_a.clone();
        seeds_b[point_idx] = self.prg.new_seed();

        let mut bits_b = bits_a.clone();
        bits_b[point_idx] = 1 - bits_b[point_idx];

        let encoded_msg =
            self.prg.eval(&seeds_a[point_idx]) ^ self.prg.eval(&seeds_b[point_idx]) ^ msg;

        vec![
            Self::Key::new(encoded_msg.clone(), bits_a, seeds_a),
            Self::Key::new(encoded_msg, bits_b, seeds_b),
        ]
    }

    fn gen_empty(&self) -> Vec<Self::Key> {
        let seeds: Vec<_> = repeat_with(|| self.prg.new_seed())
            .take(self.num_points)
            .collect();
        let bits: Vec<_> = repeat_with(|| thread_rng().gen_range(0..2))
            .take(self.num_points)
            .collect();
        let encoded_msg = self.prg.eval(&self.prg.new_seed()); // random message

        vec![Self::Key::new(encoded_msg, bits, seeds); 2]
    }

    /// evaluates the DPF on a given PRGKey and outputs the resulting data
    fn eval(&self, key: Self::Key) -> Vec<P::Output> {
        let msg_ref = Arc::new(key.encoded_msg);
        key.seeds
            .iter()
            .zip(key.bits.iter())
            .map(|(seed, bits)| {
                if *bits == 1 {
                    self.prg.eval(seed) ^ msg_ref.clone()
                } else {
                    self.prg.eval(seed)
                }
            })
            .collect()
    }

    /// combines the results produced by running eval on both keys
    fn combine(&self, parts: Vec<Vec<P::Output>>) -> Vec<P::Output> {
        let mut parts = parts.into_iter();
        let mut res = parts.next().expect("Need at least one part to combine.");
        for part in parts {
            for (x, y) in res.iter_mut().zip(part.into_iter()) {
                *x ^= y;
            }
        }
        res
    }
}

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

#[cfg(any(test, feature = "testing"))]
impl<P: Arbitrary + 'static> Arbitrary for Construction<P> {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        const MAX_NUM_POINTS: usize = 10;
        (any::<P>(), 1..=MAX_NUM_POINTS)
            .prop_map(move |(prg, num_points)| Construction::new(prg, num_points))
            .boxed()
    }
}

// #[cfg(any(test, feature = "testing"))]
// pub fn data_with_dpf<D>() -> impl Strategy<Value = (Bytes, D)>
// where
//     D: DPF<Message = Bytes> + Arbitrary + Clone,
// {
//     any::<D>().prop_flat_map(|dpf| {
//         (
//             any_with::<Bytes>(dpf.null_message().len().into()),
//             Just(dpf),
//         )
//     })
// }
