#![allow(dead_code)]
use crate::proto;

pub mod accumulator;
pub mod insecure;
pub mod table;
pub mod wrapper;

use accumulator::Accumulatable;

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Bytes(Vec<u8>);

impl From<Vec<u8>> for Bytes {
    fn from(other: Vec<u8>) -> Self {
        Bytes(other)
    }
}

impl Into<Vec<u8>> for Bytes {
    fn into(self) -> Vec<u8> {
        self.0
    }
}

pub enum ChannelKeyWrapper {
    Insecure(usize, String),
}

pub trait Protocol {
    // TODO: remove From/Into/Sync/Send bounds, as they are handled by ProtocolWrapper
    type ChannelKey: Sync + Send;
    type WriteToken: Sync + Send + From<proto::WriteToken> + Into<proto::WriteToken>;
    type AuditShare: Sync + Send + From<proto::AuditShare> + Into<proto::AuditShare>;
    type Accumulator: Into<Vec<Bytes>> + Accumulatable;

    // General protocol properties
    fn num_parties(&self) -> usize;
    fn num_channels(&self) -> usize;

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

    fn new_accumulator(&self) -> Self::Accumulator;
    fn to_accumulator(&self, token: Self::WriteToken) -> Self::Accumulator;
}
