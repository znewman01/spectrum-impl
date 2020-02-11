use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Mutex;

pub trait Data: Default {
    fn combine(&mut self, other: Self);
}

impl<T> Data for Option<T> {
    fn combine(&mut self, other: Option<T>) {
        if let Some(value) = other {
            self.replace(value);
        }
    }
}

pub struct Aggregator<D> {
    state: Vec<(Mutex<D>, AtomicUsize)>,
}

impl<D> Aggregator<D>
where
    D: Data + Clone,
{
    pub fn new(num_channels: usize) -> Aggregator<D> {
        let mut state = vec![];
        for _ in 0..num_channels {
            state.push((Mutex::default(), AtomicUsize::default()));
        }
        Aggregator { state }
    }

    pub async fn aggregate(&self, channel: usize, data: D) -> usize {
        let (mutex, count) = self
            .state
            .get(channel)
            .expect("Requested channel should be in-bounds.");
        let mut lock = mutex.lock().await;
        (*lock).combine(data);
        count.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub async fn get(&self, channel: usize) -> D {
        let (mutex, _) = self
            .state
            .get(channel)
            .expect("Requested channel should be in-bounds.");
        let lock = mutex.lock().await;
        lock.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const NUM_CHANNELS: usize = 10;

    type MyData = u8;

    impl Data for MyData {
        fn combine(&mut self, other: MyData) {
            (*self) += other;
        }
    }

    #[should_panic]
    #[tokio::test]
    async fn test_aggregator_get_out_of_bounds() {
        let aggregator = Aggregator::<MyData>::new(NUM_CHANNELS);
        aggregator.get(NUM_CHANNELS + 1).await;
    }

    #[should_panic]
    #[tokio::test]
    async fn test_aggregator_aggregate_out_of_bounds() {
        let aggregator = Aggregator::<MyData>::new(NUM_CHANNELS);
        aggregator
            .aggregate(NUM_CHANNELS + 1, MyData::default())
            .await;
    }

    #[tokio::test]
    async fn test_aggregator_get_empty() {
        let aggregator = Aggregator::<MyData>::new(NUM_CHANNELS);

        for channel in 0..NUM_CHANNELS {
            assert_eq!(aggregator.get(channel).await, 0);
        }
    }

    #[tokio::test]
    async fn test_aggregator_aggregate_identity() {
        let aggregator = Aggregator::<MyData>::new(NUM_CHANNELS);

        for channel in 0..NUM_CHANNELS {
            for _ in 0..channel {
                aggregator.aggregate(channel, MyData::default()).await;
            }
        }

        for channel in 0..NUM_CHANNELS {
            assert_eq!(aggregator.get(channel).await, 0);
        }
    }

    #[tokio::test]
    async fn test_aggregator_aggregate_unit() {
        let aggregator = Aggregator::<MyData>::new(NUM_CHANNELS);

        for channel in 0..NUM_CHANNELS {
            for _ in 0..channel {
                aggregator.aggregate(channel, 1).await;
            }
        }

        for channel in 0..NUM_CHANNELS {
            assert_eq!(aggregator.get(channel).await, channel as u8);
        }
    }
}
