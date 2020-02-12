use std::ops::{Deref, DerefMut};
use tokio::sync::RwLock;

pub trait Foldable: Default {
    type Item;

    fn combine(&mut self, other: Self::Item);
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

    type MyData = u8;

    impl Foldable for MyData {
        type Item = MyData;

        fn combine(&mut self, other: Self::Item) {
            (*self) += other;
        }
    }

    #[tokio::test]
    async fn test_accumulator_get_empty() {
        let accumulator = Accumulator::<MyData>::default();

        assert_eq!(accumulator.get().await, 0);
    }

    #[tokio::test]
    async fn test_accumulator_accumulate_identity() {
        let accumulator = Accumulator::<MyData>::default();

        accumulator.accumulate(MyData::default()).await;

        assert_eq!(accumulator.get().await, 0);
    }

    #[tokio::test]
    async fn test_accumulator_accumulate_unit() {
        let accumulator = Accumulator::<MyData>::default();
        let count = 10;

        for _ in 0..count {
            accumulator.accumulate(1).await;
        }

        assert_eq!(accumulator.get().await, count as u8);
    }

    #[tokio::test]
    async fn test_accumulator_vec() {
        let data: Vec<u8> = vec![0, 0, 0];
        let accumulator = Accumulator::new(data);

        let data = vec![0, 1, 2];
        accumulator.accumulate(data.clone()).await;
        accumulator.accumulate(data).await;

        assert_eq!(accumulator.get().await, vec![0, 2, 4]);
    }
}
