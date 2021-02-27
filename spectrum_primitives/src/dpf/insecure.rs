use super::DPF;

use std::marker::PhantomData;

#[derive(Debug, Clone)]
struct Construction<M> {
    num_points: usize,
    num_keys: usize,
    phantom: PhantomData<M>,
}

impl<M> Construction<M> {
    fn new(num_points: usize, num_keys: usize) -> Self {
        Construction::<M> {
            num_points,
            num_keys,
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

    fn num_points(&self) -> usize {
        self.num_points
    }

    fn num_keys(&self) -> usize {
        self.num_keys
    }

    fn null_message(&self) -> Self::Message {
        M::default()
    }

    fn msg_size(&self) -> usize {
        1
    }

    fn gen(&self, msg: Self::Message, idx: usize) -> Vec<Self::Key> {
        assert!(idx <= self.num_points);
        let mut keys = vec![None; self.num_keys() - 1];
        keys.push(Some((msg, idx)));
        keys
    }

    fn gen_empty(&self) -> Vec<Self::Key> {
        vec![None; self.num_keys()]
    }

    fn eval(&self, key: Self::Key) -> Vec<Self::Message> {
        let mut acc = vec![Self::Message::default(); self.num_points()];
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
mod tests {
    use super::*;

    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    struct Message {}

    impl Arbitrary for Message {
        type Parameters = usize;
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: usize) -> Self::Strategy {
            Just(Message::default()).boxed()
        }
    }

    check_dpf!(Construction<Message>);
}
