use crate::{
    crypto::field::FieldElement,
    protocols::{
        insecure,
        secure::{self, ConcreteVdpf},
    },
};

use std::convert::TryFrom;
use std::fmt::Debug;

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

pub enum ProtocolWrapper {
    Secure(secure::SecureProtocol<ConcreteVdpf>),
    Insecure(insecure::InsecureProtocol),
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::convert::TryInto;

    proptest! {
        #[test]
        fn test_secure_channel_key_proto_roundtrips(key in any::<secure::ChannelKey<ConcreteVdpf>>()) {
            let wrapped: ChannelKeyWrapper = key.clone().into();
            assert_eq!(key, wrapped.try_into().unwrap());
        }
    }
}
