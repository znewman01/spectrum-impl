#![allow(dead_code)]
pub mod accumulator;
pub mod insecure;
pub mod table;
// TODO(zjn): adapter to/from protos

trait Accumulatable {
    fn accumulate(&mut self, rhs: Self);

    fn new(size: usize) -> Self;
}

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

trait Protocol {
    type Message: Default;
    type ChannelKey;
    type WriteToken;
    type AuditShare;
    type Accumulator: Into<Vec<Self::Message>> + Accumulatable;

    // General protocol properties
    fn num_parties(&self) -> usize;
    fn num_channels(&self) -> usize;

    // Client algorithms
    fn broadcast(&self, message: Self::Message, key: Self::ChannelKey) -> Vec<Self::WriteToken>;
    fn null_broadcast(&self) -> Vec<Self::WriteToken>;

    // Server algorithms
    fn gen_audit(
        &self,
        keys: &[Self::ChannelKey],
        token: Self::WriteToken,
    ) -> Vec<Self::AuditShare>;
    fn check_audit(&self, tokens: Vec<Self::AuditShare>) -> bool;

    fn new_accumulator(&self) -> Self::Accumulator {
        Self::Accumulator::new(self.num_channels())
    }

    fn to_accumulator(&self, token: Self::WriteToken) -> Self::Accumulator;
}
