use crate::proto::{AuditShare, WriteToken};
use crate::protocols::{ChannelKeyWrapper, Protocol};

use std::convert::TryFrom;
use std::convert::TryInto;
use std::fmt::Debug;

type Bytes = Vec<u8>; // as in prost

pub trait ProtocolWrapper {
    fn broadcast(&self, message: Bytes, key: ChannelKeyWrapper) -> Vec<WriteToken>;
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
    P::Message: Into<Bytes> + From<Bytes>,
    P::ChannelKey: TryFrom<ChannelKeyWrapper> + Into<ChannelKeyWrapper>,
    <P::ChannelKey as TryFrom<ChannelKeyWrapper>>::Error: Debug,
    P::WriteToken: TryFrom<WriteToken> + Into<WriteToken>,
    <P::WriteToken as TryFrom<WriteToken>>::Error: Debug,
    P::AuditShare: TryFrom<AuditShare> + Into<AuditShare>,
    <P::AuditShare as TryFrom<AuditShare>>::Error: Debug,
{
    fn broadcast(&self, message: Bytes, key: ChannelKeyWrapper) -> Vec<WriteToken> {
        let message = message.into();
        let key = key.try_into().unwrap();
        self.broadcast(message, key)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    fn null_broadcast(&self) -> Vec<WriteToken> {
        self.null_broadcast().into_iter().map(Into::into).collect()
    }

    fn gen_audit(&self, keys: Vec<ChannelKeyWrapper>, token: WriteToken) -> Vec<AuditShare> {
        let keys: Result<Vec<P::ChannelKey>, _> = keys.into_iter().map(TryInto::try_into).collect();
        let shares = self.gen_audit(&keys.unwrap(), &token.try_into().unwrap());
        shares.into_iter().map(Into::into).collect()
    }

    fn check_audit(&self, tokens: Vec<AuditShare>) -> bool {
        let tokens: Result<Vec<P::AuditShare>, _> =
            tokens.into_iter().map(TryInto::try_into).collect();
        self.check_audit(tokens.unwrap())
    }
}
