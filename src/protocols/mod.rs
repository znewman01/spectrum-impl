#![allow(dead_code)]
use crate::proto;

pub mod accumulator;
pub mod insecure;
pub mod table;
pub mod wrapper;

use accumulator::Accumulatable;

pub enum ChannelKeyWrapper {
    Insecure,
}

pub trait Protocol {
    type Message: Default;
    // TODO: remove From/Into/Sync/Send bounds, as they are handled by ProtocolWrapper
    type ChannelKey: Sync + Send;
    type WriteToken: Sync + Send + From<proto::WriteToken> + Into<proto::WriteToken>;
    type AuditShare: Sync + Send + From<proto::AuditShare> + Into<proto::AuditShare>;
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
        token: &Self::WriteToken,
    ) -> Vec<Self::AuditShare>;
    fn check_audit(&self, tokens: Vec<Self::AuditShare>) -> bool;

    fn new_accumulator(&self) -> Self::Accumulator {
        Self::Accumulator::new(self.num_channels())
    }

    fn to_accumulator(&self, token: Self::WriteToken) -> Self::Accumulator;
}
