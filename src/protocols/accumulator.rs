use std::ops::{Deref, DerefMut};
use tokio::sync::RwLock;

use crate::protocols::Bytes;

pub trait Accumulatable {
    fn accumulate(&mut self, rhs: Self);

    fn new(size: usize) -> Self;
}

// TODO(zjn): sort through this mess
impl<T> Accumulatable for Vec<T>
where
    T: Accumulatable + Default + Clone,
{
    fn accumulate(&mut self, rhs: Vec<T>) {
        assert_eq!(self.len(), rhs.len());
        for (this, that) in self.iter_mut().zip(rhs.into_iter()) {
            this.accumulate(that);
        }
    }

    fn new(size: usize) -> Self {
        vec![Default::default(); size]
    }
}

pub trait Foldable: Default {
    type Item;

    fn combine(&mut self, other: Self::Item);
}

impl Foldable for u8 {
    type Item = u8;

    fn combine(&mut self, other: u8) {
        (*self) += other;
    }
}

impl Foldable for Bytes {
    type Item = Bytes;

    fn combine(&mut self, other: Bytes) {
        self.0.extend(other.0) // TODO(zjn) should be XOR
    }
}
// TODO(zjn): get rid of me, replace with bit data
impl<T> Foldable for Option<T> {
    type Item = Option<T>;

    fn combine(&mut self, other: Option<T>) {
        if let Some(value) = other {
            self.replace(value);
        }
    }
}

impl<T> Foldable for Vec<T>
where
    T: Foldable,
{
    type Item = Vec<T::Item>;

    fn combine(&mut self, other: Vec<T::Item>) {
        assert_eq!(self.len(), other.len());
        for (this, that) in self.iter_mut().zip(other.into_iter()) {
            this.combine(that);
        }
    }
}

#[derive(Default)]
pub struct Accumulator<D> {
    lock: RwLock<(D, usize)>,
}

impl<D> Accumulator<D>
where
    D: Foldable + Clone,
{
    pub fn new(accum: D) -> Accumulator<D> {
        let data = (accum, 0 as usize);
        Accumulator {
            lock: RwLock::new(data),
        }
    }

    pub async fn accumulate(&self, data: D::Item) -> usize {
        let mut lock = self.lock.write().await;
        let tuple: &mut (D, usize) = lock.deref_mut();
        let state = &mut tuple.0;
        let count = &mut tuple.1;

        state.combine(data);
        *count += 1;
        *count
    }

    pub async fn get(&self) -> D {
        let lock = self.lock.read().await;
        let (state, _) = lock.deref();
        state.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default, Debug, Clone, PartialEq, Eq)]
    struct MyData(u8);

    impl Foldable for MyData {
        type Item = MyData;

        fn combine(&mut self, other: Self::Item) {
            (*self).0 += other.0;
        }
    }

    #[tokio::test]
    async fn test_accumulator_get_empty() {
        let accumulator = Accumulator::<MyData>::default();

        assert_eq!(accumulator.get().await, MyData(0));
    }

    #[tokio::test]
    async fn test_accumulator_accumulate_identity() {
        let accumulator = Accumulator::<MyData>::default();

        accumulator.accumulate(MyData::default()).await;

        assert_eq!(accumulator.get().await, MyData(0));
    }

    #[tokio::test]
    async fn test_accumulator_accumulate_unit() {
        let accumulator = Accumulator::<MyData>::default();
        let count = 10;

        for _ in 0..count {
            accumulator.accumulate(MyData(1)).await;
        }

        assert_eq!(accumulator.get().await, MyData(count as u8));
    }

    #[tokio::test]
    async fn test_accumulator_vec() {
        let data: Vec<MyData> = vec![MyData(0); 3];
        let accumulator = Accumulator::new(data);

        let data = vec![MyData(0), MyData(1), MyData(2)];
        accumulator.accumulate(data.clone()).await;
        accumulator.accumulate(data).await;

        assert_eq!(
            accumulator.get().await,
            vec![MyData(0), MyData(2), MyData(4)]
        );
    }
}
