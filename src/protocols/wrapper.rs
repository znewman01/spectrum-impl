use crate::proto::{AuditShare, WriteToken};
use crate::protocols::{Accumulatable, Bytes, ChannelKeyWrapper, Protocol};

use std::convert::TryFrom;
use std::convert::TryInto;
use std::fmt::Debug;

pub trait AccumulatorWrapper {
    fn accumulate(&mut self, rhs: Vec<Bytes>);
}

impl<A> AccumulatorWrapper for A
where
    A: Into<Vec<Bytes>> + Accumulatable + From<Vec<Bytes>>,
{
    fn accumulate(&mut self, rhs: Vec<Bytes>) {
        self.accumulate(rhs.into());
    }
}

pub trait ProtocolWrapper {
    fn broadcast(&self, message: Bytes, key: ChannelKeyWrapper) -> Vec<WriteToken>;
    fn null_broadcast(&self) -> Vec<WriteToken>;

    fn gen_audit(&self, keys: Vec<ChannelKeyWrapper>, token: WriteToken) -> Vec<AuditShare>;
    fn check_audit(&self, tokens: Vec<AuditShare>) -> bool;

    fn new_accumulator(&self) -> Box<dyn AccumulatorWrapper>;
    fn expand_write_token(&self, token: WriteToken) -> Vec<Bytes>;
}

// some of these bounds are redundant:
// - we know P::Accumulator: Into<Vec<P::Message>> and P::Message: Into<Bytes>, so we must be able to convert directly P::Accumulator: Into<Vec<Bytes>>
// - similarly for P::Accumulator: From<P::WriteToken> and P::WriteToken: From<WriteToken>
impl<P> ProtocolWrapper for P
where
    P: Protocol,
    P::ChannelKey: TryFrom<ChannelKeyWrapper> + Into<ChannelKeyWrapper>,
    <P::ChannelKey as TryFrom<ChannelKeyWrapper>>::Error: Debug,
    P::WriteToken: TryFrom<WriteToken> + Into<WriteToken>,
    <P::WriteToken as TryFrom<WriteToken>>::Error: Debug,
    P::AuditShare: TryFrom<AuditShare> + Into<AuditShare>,
    <P::AuditShare as TryFrom<AuditShare>>::Error: Debug,
    P::Accumulator: 'static + Into<Vec<Bytes>> + From<Vec<Bytes>>,
{
    fn broadcast(&self, message: Bytes, key: ChannelKeyWrapper) -> Vec<WriteToken> {
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

    fn new_accumulator(&self) -> Box<dyn AccumulatorWrapper> {
        Box::new(self.new_accumulator())
    }

    fn expand_write_token(&self, token: WriteToken) -> Vec<Bytes> {
        self.to_accumulator(token.into()).into()
    }
}
