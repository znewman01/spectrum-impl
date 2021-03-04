// s-DPF (i.e. num_keys = s > 2) based on any seed-homomorphic PRG G(.).
use super::DPF;
use crate::prg::SeedHomomorphicPRG;
use crate::prg::PRG;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::iter::repeat_with;
use std::ops;

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Construction<P> {
    prg: P,
    num_points: usize,
    num_keys: usize,
}

impl<P> Construction<P> {
    pub fn new(prg: P, num_points: usize, num_keys: usize) -> Construction<P> {
        Construction {
            prg,
            num_points,
            num_keys,
        }
    }
}

pub struct Key<M, S> {
    encoded_msg: M, //P::Output,
    bits: Vec<u8>,
    seeds: Vec<S>, // Vec<<P as PRG>::Seed>,
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
    P: PRG + Clone + SeedHomomorphicPRG,
    P::Seed:
        Clone + PartialEq + Eq + Debug + ops::Sub<Output = P::Seed> + ops::Add + ops::AddAssign,
    P::Output: Clone + PartialEq + Eq + Debug,
{
    type Key = Key<P::Output, P::Seed>;
    type Message = P::Output;

    fn num_points(&self) -> usize {
        self.num_points
    }

    fn num_keys(&self) -> usize {
        self.num_keys
    }

    fn msg_size(&self) -> usize {
        self.prg.output_size()
    }

    fn null_message(&self) -> Self::Message {
        self.prg.null_output()
    }

    /// generate new instance of PRG based DPF with two DPF keys
    fn gen(&self, msg: Self::Message, point_idx: usize) -> Vec<Self::Key> {
        let mut keys = self.gen_empty();

        // generate a new random seed for the specified index
        let special_seed = self.prg.new_seed();

        keys[0].seeds[point_idx] += special_seed.clone();
        keys[0].bits[point_idx] ^= 1;

        // add message to the set of PRG outputs to "combine" together in the next step
        // encoded message G(S*) ^ msg
        let neg = self.prg.null_seed() - special_seed;
        let encoded_msg = self.prg.combine_outputs(&[&msg, &self.prg.eval(&neg)]);

        // update the encoded message in the keys
        for key in keys.iter_mut() {
            key.encoded_msg = encoded_msg.clone();
        }

        keys
    }

    fn gen_empty(&self) -> Vec<Self::Key> {
        // vector of seeds for each key
        let mut seeds: Vec<Vec<_>> = repeat_with(|| {
            repeat_with(|| self.prg.new_seed())
                .take(self.num_points)
                .collect()
        })
        .take(self.num_keys - 1)
        .collect();

        // want all seeds to cancel out; set last seed to be negation of all former seeds
        let mut last_seed_vec = vec![self.prg.null_seed(); self.num_points];
        for seed_vec in seeds.iter() {
            for (a, b) in last_seed_vec.iter_mut().zip(seed_vec.iter()) {
                *a = a.clone() - b.clone();
            }
        }
        seeds.push(last_seed_vec);

        // vector of bits for each key
        let mut bits: Vec<Vec<_>> =
            repeat_with(|| repeat_with(|| 0).take(self.num_points).collect())
                .take(self.num_keys)
                .collect();

        // want bits to cancel out; set the last bit to be the xor of all the other bits
        let mut last_bit_vec = vec![0; self.num_points];
        for bit_vec in bits.iter() {
            for (a, b) in last_bit_vec.iter_mut().zip(bit_vec.iter()) {
                *a ^= b
            }
        }

        bits.push(last_bit_vec);

        let encoded_msg = self.prg.eval(&self.prg.new_seed()); // psuedo random message

        seeds
            .iter()
            .zip(bits.iter())
            .map(|(seed_vec, bit_vec)| {
                Key::new(encoded_msg.clone(), bit_vec.clone(), seed_vec.clone())
            })
            .collect()
    }

    /// evaluates the DPF on a given PRGKey and outputs the resulting data
    fn eval(&self, key: Self::Key) -> Vec<Self::Message> {
        for (seed, bits) in key.seeds.iter().zip(key.bits.iter()) {
            if *bits == 1 {
                self.prg
                    .combine_outputs(&[&key.encoded_msg, &self.prg.eval(seed)]);
            } else {
                self.prg.eval(seed);
            }
        }
        vec![]
    }

    /// combines the results produced by running eval on both keys
    ///
    /// combine([[a, b], [c, d], [e, f]]) == [a + c + e, b + d + f]
    fn combine(&self, parts: Vec<Vec<Self::Message>>) -> Vec<Self::Message> {
        parts
            .into_iter()
            .reduce(|part1, part2| {
                part1
                    .iter()
                    .zip(part2.iter())
                    .map(|(x, y)| self.prg.combine_outputs(&[x, y]))
                    .collect()
            })
            .expect("parts should be nonempty")
    }
}

/// Test helpers
#[cfg(any(test, feature = "testing"))]
mod testing {
    pub const MAX_NUM_POINTS: usize = 5;
    pub const MAX_NUM_KEYS: usize = 3;
}

#[cfg(any(test, feature = "testing"))]
impl<P: Arbitrary + 'static> Arbitrary for Construction<P> {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use testing::*;
        (any::<P>(), 2..=MAX_NUM_KEYS, 1..=MAX_NUM_POINTS)
            .prop_map(move |(prg, num_keys, num_points)| {
                Construction::new(prg, num_points, num_keys)
            })
            .boxed()
    }
}
