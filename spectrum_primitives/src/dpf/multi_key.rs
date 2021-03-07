// s-DPF (i.e. keys = s > 2) based on any seed-homomorphic PRG G(.).
use std::fmt::Debug;
use std::iter::repeat_with;

use serde::{Deserialize, Serialize};

use super::Dpf;
use crate::algebra::{Field, SpecialExponentMonoid};
use crate::lss::Shareable;
use crate::prg::Prg;
use crate::prg::SeedHomomorphicPrg;

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Construction<P> {
    prg: P,
    points: usize,
    keys: usize,
}

impl<P> Construction<P> {
    pub fn new(prg: P, points: usize, keys: usize) -> Construction<P> {
        Construction { prg, points, keys }
    }
}

pub struct Key<M, S> {
    pub(in crate) encoded_msg: M, //P::Output,
    pub(in crate) bits: Vec<S>,
    pub(in crate) seeds: Vec<S>,
}

impl<M, S> Key<M, S> {
    fn new(encoded_msg: M, bits: Vec<S>, seeds: Vec<S>) -> Self {
        assert_eq!(bits.len(), seeds.len());
        Key {
            encoded_msg,
            bits,
            seeds,
        }
    }
}

impl<P> Dpf for Construction<P>
where
    P: Prg + Clone + SeedHomomorphicPrg,
    P::Seed: Clone + PartialEq + Eq + Debug + Field + Shareable<Share = P::Seed>,
    P::Output: Clone + PartialEq + Eq + Debug + SpecialExponentMonoid<Exponent = P::Seed>,
{
    type Key = Key<P::Output, P::Seed>;
    type Message = P::Output;

    fn points(&self) -> usize {
        self.points
    }

    fn keys(&self) -> usize {
        self.keys
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
        let seed = P::new_seed();

        keys[0].seeds[point_idx] = keys[0].seeds[point_idx].clone() - seed.clone();
        keys[0].bits[point_idx] = keys[0].bits[point_idx].clone() + P::Seed::one();

        // add message to the set of PRG outputs to "combine" together in the next step
        // encoded message G(S*) ^ msg
        let encoded_msg = self.prg.combine_outputs(&[&msg, &self.prg.eval(&seed)]);

        // update the encoded message in the keys
        keys.iter_mut()
            .for_each(|k| k.encoded_msg = encoded_msg.clone());

        keys
    }

    fn gen_empty(&self) -> Vec<Self::Key> {
        // share all-zero seeds
        let seeds: Vec<_> = repeat_with(P::null_seed).take(self.points).collect();
        let seed_shares = seeds.share(self.keys);
        // share all-zero
        let bits: Vec<_> = repeat_with(P::null_seed).take(self.points).collect();
        let bit_shares = bits.share(self.keys);
        // (pseudo)random  message
        let encoded_msg = self.prg.eval(&P::new_seed());

        Iterator::zip(seed_shares.into_iter(), bit_shares.into_iter())
            .map(|(s, b)| Key::new(encoded_msg.clone(), b, s))
            .collect()
    }

    /// evaluates the DPF on a given PRGKey and outputs the resulting data
    fn eval(&self, key: Self::Key) -> Vec<Self::Message> {
        Iterator::zip(key.seeds.iter(), key.bits.iter().cloned())
            .map(|(seed, bit)| {
                self.prg
                    .combine_outputs(&[&key.encoded_msg.pow(bit), &self.prg.eval(seed)])
            })
            .collect()
    }

    /// combines the results produced by running eval on both keys
    /// combine([[a, b], [c, d], [e, f]]) == [a + c + e, b + d + f]
    fn combine(&self, parts: Vec<Vec<Self::Message>>) -> Vec<Self::Message> {
        parts
            .into_iter()
            .reduce(|part1, part2| {
                Iterator::zip(part1.iter(), part2.iter())
                    .map(|(x, y)| self.prg.combine_outputs(&[x, y]))
                    .collect()
            })
            .expect("parts should be nonempty")
    }
}

/// Test helpers
#[cfg(any(test, feature = "testing"))]
mod testing {
    pub const MAX_POINTS: usize = 5;
    pub const MAX_KEYS: usize = 3;
}

#[cfg(any(test, feature = "testing"))]
impl<P: Arbitrary + 'static> Arbitrary for Construction<P> {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use testing::*;
        (any::<P>(), 2..=MAX_KEYS, 1..=MAX_POINTS)
            .prop_map(move |(prg, keys, points)| Construction::new(prg, points, keys))
            .boxed()
    }
}
