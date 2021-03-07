//! Spectrum implementation.
use crate::dpf::DPF;

use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::marker::PhantomData;

#[cfg(any(test))]
use proptest::prelude::*;
#[cfg(any(test))]
use proptest_derive::Arbitrary;

// #[cfg_attr(test, derive(Arbitrary))]
// #[derive(Clone, PartialEq, Debug)]
// pub struct FieldToken<F>
// where
//     F: Shareable,
// {
//     pub bit: F::Share,
//     pub seed: F::Share,
//     pub data: Bytes,
// }
//
// impl<F> FieldToken<F>
// where
//     F: Shareable,
// {
//     pub fn new(bit: F::Share, seed: F::Share, data: Bytes) -> Self {
//         Self { bit, seed, data }
//     }
// }
//
// #[cfg_attr(test, derive(Arbitrary))]
// #[derive(Clone, PartialEq, Debug)]
// pub struct FieldProof<F> {
//     bit: F,
//     seed: F,
// }
//
// impl<F> FieldProof<F> {
//     fn new(bit: F, seed: F) -> Self {
//         Self { bit, seed }
//     }
// }
//
// impl<F> Shareable for FieldProof<F>
// where
//     F: Shareable<Share = F> + Clone,
// {
//     type Share = Self;
//
//     fn share(self, len: usize) -> Vec<Self::Share> {
//         Iterator::zip(
//             self.bit.share(len).into_iter(),
//             self.seed.share(len).into_iter(),
//         )
//         .map(|(bit, seed)| Self::new(bit, seed))
//         .collect()
//     }
//
//     fn recover(shares: Vec<Self::Share>) -> Self {
//         let bit = F::recover(shares.iter().cloned().map(|proof| proof.bit).collect());
//         let seed = F::recover(shares.iter().cloned().map(|proof| proof.seed).collect());
//         Self { bit, seed }
//     }
// }
//
// impl<F> LinearlyShareable<FieldProof<F>> for FieldProof<F> where
//     FieldProof<F>: Shareable<Share = FieldProof<F>>
// {
// }
//
// #[cfg_attr(test, derive(Arbitrary))]
// #[derive(Clone, PartialEq, Debug)]
// pub struct FieldProofShare<F>
// where
//     F: Shareable,
// {
//     pub bit: F::Share,
//     pub seed: F::Share,
// }
//
// impl<F> FieldProofShare<F>
// where
//     F: Shareable,
// {
//     pub fn new(bit: F::Share, seed: F::Share) -> Self {
//         Self { bit, seed }
//     }
//
//     pub fn share(bit_proof: F, seed_proof: F, len: usize) -> Vec<FieldProofShare<F>> {
//         let bits = bit_proof.share(len);
//         let seeds = seed_proof.share(len);
//         bits.into_iter()
//             .zip(seeds.into_iter())
//             .map(|(bit, seed)| FieldProofShare::new(bit, seed))
//             .collect()
//     }
// }

#[cfg_attr(test, derive(Arbitrary))]
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct FieldVDPF<D, F> {
    dpf: D,
    phantom: PhantomData<F>,
}

impl<D, F> FieldVDPF<D, F> {
    pub fn new(dpf: D) -> Self {
        FieldVDPF {
            dpf,
            phantom: Default::default(),
        }
    }
}

// Pass through DPF methods
impl<D: DPF, F> DPF for FieldVDPF<D, F> {
    type Key = D::Key;
    type Message = D::Message;

    fn points(&self) -> usize {
        self.dpf.points()
    }

    fn keys(&self) -> usize {
        self.dpf.keys()
    }

    fn msg_size(&self) -> usize {
        self.dpf.msg_size()
    }

    fn null_message(&self) -> Self::Message {
        self.dpf.null_message()
    }

    fn gen(&self, msg: Self::Message, idx: usize) -> Vec<Self::Key> {
        self.dpf.gen(msg, idx)
    }

    fn gen_empty(&self) -> Vec<Self::Key> {
        self.dpf.gen_empty()
    }

    fn eval(&self, key: Self::Key) -> Vec<Self::Message> {
        self.dpf.eval(key)
    }

    fn combine(&self, parts: Vec<Vec<Self::Message>>) -> Vec<Self::Message> {
        self.dpf.combine(parts)
    }
}

#[cfg(any(test, feature = "testing"))]
mod testing {
    use proptest::prelude::*;

    pub(super) fn hashes() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(super::any::<u8>(), 32)
    }
}