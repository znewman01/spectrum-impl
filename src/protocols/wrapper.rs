use crate::proto::{AuditShare, WriteToken};
use crate::protocols::{ChannelKeyWrapper, Protocol};

use std::convert::TryFrom;
use std::convert::TryInto;
use std::fmt::Debug;

pub trait ProtocolWrapper {
    // fn broadcast(&self, message: Self::Message, key: Self::ChannelKey) -> Vec<elf::WriteToken>;
    fn null_broadcast(&self) -> Vec<WriteToken>;

    fn gen_audit(&self, keys: Vec<ChannelKeyWrapper>, token: WriteToken) -> Vec<AuditShare>;
    fn check_audit(&self, tokens: Vec<AuditShare>) -> bool;

    // fn new_accumulator(&self) -> Self::Accumulator {
    //     Self::Accumulator::new(self.num_channels())
    // }

    // fn to_accumulator(&self, token: Self::WriteToken) -> Self::Accumulator;
}

impl<P> ProtocolWrapper for P
where
    P: Protocol,
    <P::ChannelKey as TryFrom<ChannelKeyWrapper>>::Error: Debug,
{
    fn null_broadcast(&self) -> Vec<WriteToken> {
        self.null_broadcast().into_iter().map(Into::into).collect()
    }

    fn gen_audit(&self, keys: Vec<ChannelKeyWrapper>, token: WriteToken) -> Vec<AuditShare> {
        let keys: Result<Vec<P::ChannelKey>, _> = keys.into_iter().map(TryInto::try_into).collect();
        let shares = self.gen_audit(&keys.unwrap(), &token.into());
        shares.into_iter().map(Into::into).collect()
    }

    fn check_audit(&self, tokens: Vec<AuditShare>) -> bool {
        let tokens = tokens.into_iter().map(Into::into).collect();
        self.check_audit(tokens)
    }
}
