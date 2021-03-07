use super::DPF;

use std::marker::PhantomData;

#[derive(Debug, Clone)]
pub struct Construction<M> {
    points: usize,
    keys: usize,
    phantom: PhantomData<M>,
}

impl<M> Construction<M> {
    fn new(points: usize, keys: usize) -> Self {
        Construction::<M> {
            points,
            keys,
            phantom: PhantomData::default(),
        }
    }
}

impl<M> DPF for Construction<M>
where
    M: Default + Clone + PartialEq + Eq,
{
    type Key = Option<(M, usize)>;
    type Message = M;

    fn points(&self) -> usize {
        self.points
    }

    fn keys(&self) -> usize {
        self.keys
    }

    fn null_message(&self) -> Self::Message {
        M::default()
    }

    fn msg_size(&self) -> usize {
        1
    }

    fn gen(&self, msg: Self::Message, idx: usize) -> Vec<Self::Key> {
        assert!(idx <= self.points);
        let mut keys = vec![None; self.keys() - 1];
        keys.push(Some((msg, idx)));
        keys
    }

    fn gen_empty(&self) -> Vec<Self::Key> {
        vec![None; self.keys()]
    }

    fn eval(&self, key: Self::Key) -> Vec<Self::Message> {
        let mut acc = vec![Self::Message::default(); self.points()];
        if let Some((msg, idx)) = key {
            acc[idx] = msg;
        }
        acc
    }

    fn combine(&self, parts: Vec<Vec<Self::Message>>) -> Vec<Self::Message> {
        parts
            .into_iter()
            .reduce(|a, b| {
                a.into_iter()
                    .zip(b.into_iter())
                    .map(|(a, b)| if a != M::default() { a } else { b })
                    .collect()
            })
            .unwrap()
    }
}

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

#[cfg(any(test, feature = "testing"))]
impl<M: std::fmt::Debug> Arbitrary for Construction<M> {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        ((1..10usize), (2..10usize))
            .prop_map(|(points, keys)| Construction::<M>::new(points, keys))
            .boxed()
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Message {}

#[cfg(test)]
impl Arbitrary for Message {
    type Parameters = usize;
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: usize) -> Self::Strategy {
        Just(Message::default()).boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Message {
        pub fn len(&self) -> usize {
            1
        }
    }

    check_dpf!(Construction<Message>);
}
