use std::iter::repeat_with;

use spectrum_primitives::{Bytes, ElementVector, Group};

/// Something that can be accumulated.
///
/// Basically, a parameterized commutative monoid. For example, the parameter
/// might be the length.
pub trait Accumulatable {
    /// Parameters for creating an empty Accumultable.
    ///
    /// There's no one-size-fits-all à la Default.
    type Parameters: Copy;
    // TODO: other should be a reference?
    fn combine(&mut self, other: Self);

    fn empty(params: Self::Parameters) -> Self;

    fn params(&self) -> Self::Parameters;
}

/// Check the accumulatable properties.
///
/// Accumulatable type must implement `proptest::Arbitrary` with
/// `<T as Accumulatable>::Parameters: Into<<T as Arbitrary>::Parameters>`.
#[cfg(test)]
macro_rules! check_accumulatable {
    ($type:ty) => {
        mod accumulatable {
            #![allow(unused_imports)]
            use super::*;
            use proptest::prelude::*;
            use crate::Accumulatable;

            /// Creates identically-parameterized accumulatables.
            fn values_with_same_params(n: usize) -> impl Strategy<Value=Vec<$type>> {
                // TODO: need to rethink this. maybe should allow a Fn to map
                // Accumulatable::Parameters -> Arbitrary::Parameters? Or
                // provide parameters?
                use prop::collection::{vec, SizeRange};
                any::<$type>().prop_flat_map(move |value| {
                    vec(any_with::<$type>(value.params().into()), n)
                }).boxed()
            }

            proptest! {
                #[test]
                fn test_values_with_same_params(values in values_with_same_params(2)) {
                    prop_assert_eq!(values.len(), 2);
                    prop_assert_eq!(values[0].params(), values[1].params());
                }

                #[test]
                fn test_associative(values in values_with_same_params(3)) {
                    use std::convert::TryInto;
                    let (mut a, b, c) = (values[0].clone(), values[1].clone(), values[2].clone());
                    let (mut a2, mut b2, c2) = (a.clone(), b.clone(), c.clone());
                    //(a + b) + c
                    a.combine(b);
                    a.combine(c);
                    // a + (b + c)
                    b2.combine(c2);
                    a2.combine(b2);
                    prop_assert_eq!(a, a2);
                }

                #[test]
                fn test_commutative(values in values_with_same_params(2)) {
                    let (mut a, b) = (values[0].clone(), values[1].clone());
                    let (a2, mut b2) = (a.clone(), b.clone());
                    // a + b
                    a.combine(b);
                    // b + a
                    b2.combine(a2);
                    prop_assert_eq!(a, b2);
                }

                #[test]
                fn test_empty(mut values in values_with_same_params(1)) {
                    let a = values.pop().expect("expected 1");
                    let empty = <$type as Accumulatable>::empty(a.params());
                    prop_assert_eq!(<$type as Accumulatable>::empty(a.params()), empty.clone(), "empty should be const");
                    // a + 0 == a
                    let mut acc = a.clone();
                    acc.combine(empty.clone());
                    prop_assert_eq!(acc, a.clone());
                }
                }
        }
    };
}

impl Accumulatable for Bytes {
    type Parameters = usize;

    fn combine(&mut self, other: Bytes) {
        *self ^= &other;
    }

    fn empty(length: usize) -> Self {
        Bytes::empty(length.into())
    }

    fn params(&self) -> usize {
        self.len()
    }
}

#[cfg(test)]
mod bytes {
    use super::*;
    check_accumulatable!(Bytes);
}

impl<G> Accumulatable for ElementVector<G>
where
    G: Group + Clone,
{
    type Parameters = Option<usize>;

    fn combine(&mut self, other: Self) {
        *self ^= other;
    }
    fn empty(length: Option<usize>) -> Self {
        Self(vec![G::zero(); length.unwrap_or(1)])
    }

    fn params(&self) -> Self::Parameters {
        Some(self.0.len())
    }
}

#[cfg(test)]
mod element_vector {
    use super::*;
    use spectrum_primitives::IntsModP;
    check_accumulatable!(ElementVector<IntsModP>);
}

impl<T> Accumulatable for Vec<T>
where
    T: Accumulatable,
{
    type Parameters = (usize, T::Parameters);

    fn combine(&mut self, other: Vec<T>) {
        assert_eq!(self.len(), other.len());
        for (this, that) in self.iter_mut().zip(other.into_iter()) {
            this.combine(that);
        }
    }

    fn empty((length, subparams): (usize, T::Parameters)) -> Self {
        repeat_with(|| T::empty(subparams.clone()))
            .take(length)
            .collect()
    }

    fn params(&self) -> Self::Parameters {
        (self.len(), self[0].params())
    }
}

// TODO: this doesn't belong here
// use std::ops::{Deref, DerefMut};
// use tokio::sync::RwLock;
// pub struct Accumulator<D> {
//     lock: RwLock<(D, usize)>,
// }
//
// impl<D> Accumulator<D>
// where
//     D: Accumulatable + Clone,
// {
//     pub fn new(accum: D) -> Accumulator<D> {
//         let data = (accum, 0_usize);
//         Accumulator {
//             lock: RwLock::new(data),
//         }
//     }
//
//     pub async fn accumulate(&self, data: D) -> usize {
//         let mut lock = self.lock.write().await;
//         let tuple: &mut (D, usize) = lock.deref_mut();
//         let state = &mut tuple.0;
//         let count = &mut tuple.1;
//
//         state.combine(data);
//         *count += 1;
//         *count
//     }
//
//     pub async fn get(&self) -> D {
//         let lock = self.lock.read().await;
//         let (state, _) = lock.deref();
//         state.clone()
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[derive(Debug, Clone, PartialEq, Eq)]
//     struct MyData(u8);
//
//     impl Accumulatable for MyData {
//         type Parameters = ();
//
//         fn combine(&mut self, other: MyData) {
//             (*self).0 += other.0;
//         }
//
//         fn empty(_: ()) -> Self {
//             MyData(0)
//         }
//     }
//
//     #[tokio::test]
//     async fn test_accumulator_get_empty() {
//         let accumulator = Accumulator::new(MyData::empty(()));
//
//         assert_eq!(accumulator.get().await, MyData(0));
//     }
//
//     #[tokio::test]
//     async fn test_accumulator_accumulate_identity() {
//         let accumulator = Accumulator::new(MyData::empty(()));
//
//         accumulator.accumulate(MyData::empty(())).await;
//
//         assert_eq!(accumulator.get().await, MyData(0));
//     }
//
//     #[tokio::test]
//     async fn test_accumulator_accumulate_unit() {
//         let accumulator = Accumulator::new(MyData::empty(()));
//         let count = 10;
//
//         for _ in 0..count {
//             accumulator.accumulate(MyData(1)).await;
//         }
//
//         assert_eq!(accumulator.get().await, MyData(count as u8));
//     }
//
//     #[tokio::test]
//     async fn test_accumulator_vec() {
//         let data: Vec<MyData> = vec![MyData(0); 3];
//         let accumulator = Accumulator::new(data);
//
//         let data = vec![MyData(0), MyData(1), MyData(2)];
//         accumulator.accumulate(data.clone()).await;
//         accumulator.accumulate(data).await;
//
//         assert_eq!(
//             accumulator.get().await,
//             vec![MyData(0), MyData(2), MyData(4)]
//         );
//     }
// }
