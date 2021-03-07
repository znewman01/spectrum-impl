//! Spectrum implementation.
use crate::dpf::Dpf;

use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::marker::PhantomData;

#[cfg(any(test))]
use proptest_derive::Arbitrary;

#[cfg_attr(test, derive(Arbitrary))]
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct FieldVdpf<D, F> {
    dpf: D,
    phantom: PhantomData<F>,
}

impl<D, F> FieldVdpf<D, F> {
    pub fn new(dpf: D) -> Self {
        FieldVdpf {
            dpf,
            phantom: Default::default(),
        }
    }
}

// Pass through DPF methods
impl<D: Dpf, F> Dpf for FieldVdpf<D, F> {
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
