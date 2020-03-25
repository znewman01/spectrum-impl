use crate::proto::{AuditShare, WriteToken};
use crate::{
    crypto::field::FieldElement,
    protocols::{
        insecure,
        secure::{self, ConcreteVdpf},
        Accumulator, Bytes, Protocol,
    },
};

use std::convert::TryFrom;
use std::convert::TryInto;
use std::fmt::Debug;

pub trait ProtocolWrapper {
    fn broadcast(&self, message: Bytes, key: ChannelKeyWrapper) -> Vec<WriteToken>;
    fn null_broadcast(&self) -> Vec<WriteToken>;

    fn gen_audit(&self, keys: Vec<ChannelKeyWrapper>, token: WriteToken) -> Vec<AuditShare>;
    fn check_audit(&self, tokens: Vec<AuditShare>) -> bool;

    fn new_accumulator(&self) -> Accumulator;
    fn expand_write_token(&self, token: WriteToken) -> Accumulator;
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ChannelKeyWrapper {
    Insecure(usize, String),
    Secure(usize, FieldElement),
}

impl TryFrom<ChannelKeyWrapper> for secure::ChannelKey<ConcreteVdpf> {
    type Error = &'static str;

    fn try_from(wrapper: ChannelKeyWrapper) -> Result<Self, Self::Error> {
        if let ChannelKeyWrapper::Secure(idx, secret) = wrapper {
            Ok(secure::ChannelKey::<ConcreteVdpf>::new(idx, secret))
        } else {
            Err("Invalid channel key")
        }
    }
}

impl Into<ChannelKeyWrapper> for secure::ChannelKey<ConcreteVdpf> {
    fn into(self) -> ChannelKeyWrapper {
        ChannelKeyWrapper::Secure(self.idx, self.secret)
    }
}

impl TryFrom<ChannelKeyWrapper> for insecure::ChannelKey {
    type Error = &'static str;

    fn try_from(wrapper: ChannelKeyWrapper) -> Result<Self, Self::Error> {
        if let ChannelKeyWrapper::Insecure(idx, password) = wrapper {
            Ok(insecure::ChannelKey::new(idx, password))
        } else {
            Err("Invalid channel key")
        }
    }
}

impl Into<ChannelKeyWrapper> for insecure::ChannelKey {
    fn into(self) -> ChannelKeyWrapper {
        ChannelKeyWrapper::Insecure(self.0, self.1)
    }
}

impl<P> ProtocolWrapper for P
where
    P: Protocol,
    P::ChannelKey: TryFrom<ChannelKeyWrapper> + Into<ChannelKeyWrapper>,
    <P::ChannelKey as TryFrom<ChannelKeyWrapper>>::Error: Debug,
    P::WriteToken: TryFrom<WriteToken> + Into<WriteToken>,
    <P::WriteToken as TryFrom<WriteToken>>::Error: Debug,
    P::AuditShare: TryFrom<AuditShare> + Into<AuditShare>,
    <P::AuditShare as TryFrom<AuditShare>>::Error: Debug,
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

    fn new_accumulator(&self) -> Vec<Bytes> {
        self.new_accumulator()
    }

    fn expand_write_token(&self, token: WriteToken) -> Vec<Bytes> {
        self.to_accumulator(token.try_into().unwrap())
    }
}

pub enum ProtocolWrapper2 {
    Secure(secure::SecureProtocol<ConcreteVdpf>),
    Insecure(insecure::InsecureProtocol),
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_secure_channel_key_proto_roundtrips(key in any::<secure::ChannelKey<ConcreteVdpf>>()) {
            let wrapped: ChannelKeyWrapper = key.clone().into();
            assert_eq!(key, wrapped.try_into().unwrap());
        }
    }
}
