/// A database for private information retrieval.
pub trait Database<const NUM_SERVERS: usize> {
    type Row;
    type Query;
    type Response;

    fn len(&self) -> usize;
    fn get(&self, idx: usize) -> Self::Row;
    fn queries(idx: usize, db_size: usize) -> Result<[Self::Query; NUM_SERVERS], ()>;
    fn answer(&self, query: Self::Query) -> Result<Self::Response, ()>;
    fn combine(responses: [Self::Response; NUM_SERVERS]) -> Self::Row;
}

#[cfg(test)]
macro_rules! check_pir {
    ($type:ty, $n:expr) => {
        mod prg {
            #![allow(unused_imports)]
            use super::*;
            use crate::pir::Database;
            use proptest::prelude::*;
            use proptest::sample::Index;
            use std::convert::TryInto;

            #[test]
            fn check_bounds() {
                fn check<D: Database<$n>>() {}
                check::<$type>();
            }

            proptest! {
                /// new_seed should give random seeds.
                #[test]
                fn test_pir(db: $type, index: Index) {
                    // TODO: get rid of unwraps
                    let len = <$type as Database<$n>>::len(&db);
                    prop_assume!(len > 0);
                    let idx = index.index(len);
                    let queries = <$type as Database<$n>>::queries(idx, len).unwrap();
                    let responses = IntoIterator::into_iter(queries)
                        .map(|q| <$type as Database<$n>>::answer(&db, q).unwrap())
                        .collect::<Vec<_>>()
                        .try_into()
                        .unwrap();
                    let value = <$type as Database<$n>>::combine(responses);
                    let expected = <$type as Database<$n>>::get(&db, idx);
                    prop_assert_eq!(value, expected);
                }

            }
        }
    };
}
