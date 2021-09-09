use crate::pir::Database;
use std::rc::Rc;

#[cfg(any(test, feature = "testing"))]
use proptest_derive::Arbitrary;

#[cfg_attr(any(test, feature = "testing"), derive(Arbitrary))]
#[derive(Debug)]
struct InsecureDatabase {
    data: Rc<Vec<Vec<u8>>>,
}

impl InsecureDatabase {
    fn new(data: Rc<Vec<Vec<u8>>>) -> Self {
        assert!(data.len() > 0);
        Self { data }
    }

    fn from_vec(data: Vec<Vec<u8>>) -> Self {
        Self::new(Rc::new(data))
    }
}

impl<const N: usize> Database<N> for InsecureDatabase {
    type Row = Vec<u8>;
    type Query = usize;
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
        Ok([idx; N])
    }

    fn answer(&self, query: Self::Query) -> Result<Self::Response, ()> {
        if self.data.len() <= query {
            return Err(());
        }
        Ok(self.data[query].clone())
    }

    fn combine(responses: [Self::Response; N]) -> Self::Row {
        responses[0].clone()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    mod one_server {
        use super::*;
        check_pir!(InsecureDatabase, 1);
    }
    mod two_server {
        use super::*;
        check_pir!(InsecureDatabase, 2);
    }
    mod many_server {
        use super::*;
        check_pir!(InsecureDatabase, 3);
    }
}
