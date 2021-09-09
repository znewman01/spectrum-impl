use crate::pir::Database;
use rand::thread_rng;
use rand::Rng;
use std::convert::TryInto;
use std::iter::repeat_with;
use std::rc::Rc;

#[derive(Debug)]
/// Simple, linear PIR scheme.
///
/// For an l-row database, queries for the (i)th index are XOR secret-shares of
/// e_i (length l bit-vectors).
///
/// The response to a query is the "inner product" of the queries and the database.
/// where "multiplication" of a row (bytes) and bit is the row if the bit is 1,
/// or the 0 vector otherwise.
///
/// To recover the row, the client XORs together the responses.
pub struct LinearDatabase {
    row_len: usize,
    data: Rc<Vec<Vec<u8>>>,
}

impl LinearDatabase {
    fn new(data: Rc<Vec<Vec<u8>>>) -> Self {
        assert!(data.len() > 0);
        let row_len = data[0].len();
        for row in data.iter() {
            assert_eq!(row_len, row.len());
        }
        Self { row_len, data }
    }

    pub fn from_vec(data: Vec<Vec<u8>>) -> Self {
        Self::new(Rc::new(data))
    }

    fn empty_row(&self) -> Vec<u8> {
        vec![0u8; self.row_len]
    }
}

impl<const N: usize> Database<N> for LinearDatabase {
    type Row = Vec<u8>;
    // TODO: consider using bitvec crate or similar to pack tighter
    type Query = Vec<bool>;
    type Response = Vec<u8>;

    fn len(&self) -> usize {
        self.data.len()
    }

    fn get(&self, idx: usize) -> Self::Row {
        self.data[idx].clone()
    }

    fn queries(idx: usize, db_size: usize) -> Result<[Self::Query; N], ()> {
        if db_size <= idx {
            return Err(());
        }
        let mut rng = thread_rng();
        // N-1 random bit vectors
        let mut queries: Vec<Vec<bool>> =
            repeat_with(|| repeat_with(|| rng.gen()).take(db_size).collect())
                .take(N - 1)
                .collect();
        // last_row should make the XOR of all queries=false except at [idx].
        let mut last_row = vec![false; db_size];
        for j in 0..db_size {
            let mut parity = false;
            for i in 0..(N - 1) {
                parity ^= queries[i][j];
            }
            // Figure out the parity of all previous queries.
            // If we match that, XORing gives false.
            last_row[j] = parity
        }
        // Flip the bit at idx so that we wind up with a sharing of a one-hot vector.
        last_row[idx] = !last_row[idx];
        queries.push(last_row);
        Ok(queries.try_into().unwrap())
    }

    fn answer(&self, query: Self::Query) -> Result<Self::Response, ()> {
        if self.data.len() != query.len() {
            return Err(());
        }

        let mut answer = self.empty_row();
        for (row, bit) in self.data.iter().zip(query.iter()) {
            if *bit {
                for i in 0..self.row_len {
                    answer[i] ^= row[i]
                }
            }
        }
        Ok(answer)
    }

    fn combine(responses: [Self::Response; N]) -> Self::Row {
        let mut row = responses[0].clone();
        for response in responses.iter().skip(1) {
            assert_eq!(row.len(), response.len());
            for i in 0..row.len() {
                row[i] ^= response[i];
            }
        }
        row
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use proptest::prelude::*;

    impl Arbitrary for LinearDatabase {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            use prop::collection::vec;
            const MAX_ROW_LEN: usize = 100;
            const MAX_NUM_ROWS: usize = 10;
            (1..MAX_ROW_LEN)
                .prop_flat_map(|row_len| {
                    let row_strat = vec(any::<u8>(), row_len);
                    vec(row_strat, 1..MAX_NUM_ROWS)
                })
                .prop_map(LinearDatabase::from_vec)
                .boxed()
        }
    }

    mod two_server {
        use super::*;
        check_pir!(LinearDatabase, 2);
    }
    mod many_server {
        use super::*;
        check_pir!(LinearDatabase, 3);
    }
}
