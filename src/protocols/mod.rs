pub mod accumulator;
pub mod insecure;
pub mod wrapper;

use crate::crypto::byte_utils::Bytes;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum ChannelKeyWrapper {
    Insecure(usize, String),
}

type Accumulator = Vec<Bytes>;

pub trait Protocol {
    type ChannelKey;
    type WriteToken;
    type AuditShare;

    // General protocol properties
    fn num_parties(&self) -> usize;
    fn num_channels(&self) -> usize;
    fn message_len(&self) -> usize;

    // Client algorithms
    fn broadcast(&self, message: Bytes, key: Self::ChannelKey) -> Vec<Self::WriteToken>;
    fn null_broadcast(&self) -> Vec<Self::WriteToken>;

    // Server algorithms
    fn gen_audit(
        &self,
        keys: &[Self::ChannelKey],
        token: &Self::WriteToken,
    ) -> Vec<Self::AuditShare>;
    fn check_audit(&self, tokens: Vec<Self::AuditShare>) -> bool;

    fn new_accumulator(&self) -> Accumulator {
        vec![Bytes::empty(self.message_len()); self.num_channels()]
    }

    fn to_accumulator(&self, token: Self::WriteToken) -> Accumulator;
}
