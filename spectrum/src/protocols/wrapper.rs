use crate::{
    crypto::field::FieldElement,
    crypto::vdpf::VDPF,
    protocols::{
        insecure,
        secure::{self, BasicVdpf, MultiKeyVdpf},
        Protocol,
    },
};

use serde::{Deserialize, Serialize};

use std::convert::TryFrom;
use std::fmt::Debug;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum ChannelKeyWrapper {
    Insecure(usize, String),
    Secure(usize, FieldElement),
}

impl<V> TryFrom<ChannelKeyWrapper> for secure::ChannelKey<V>
where
    V: VDPF<AuthKey = FieldElement>,
{
    type Error = &'static str;

    fn try_from(wrapper: ChannelKeyWrapper) -> Result<Self, Self::Error> {
        if let ChannelKeyWrapper::Secure(idx, secret) = wrapper {
            Ok(secure::ChannelKey::<V>::new(idx, secret))
        } else {
            Err("Invalid channel key")
        }
    }
}

impl<V> Into<ChannelKeyWrapper> for secure::ChannelKey<V>
where
    V: VDPF<AuthKey = FieldElement>,
{
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProtocolWrapper {
    Secure(secure::SecureProtocol<BasicVdpf>),
    Insecure(insecure::InsecureProtocol),
    SecureMultiKey(secure::SecureProtocol<MultiKeyVdpf>),
}

impl From<insecure::InsecureProtocol> for ProtocolWrapper {
    fn from(protocol: insecure::InsecureProtocol) -> Self {
        Self::Insecure(protocol)
    }
}

impl From<secure::SecureProtocol<BasicVdpf>> for ProtocolWrapper {
    fn from(protocol: secure::SecureProtocol<BasicVdpf>) -> Self {
        Self::Secure(protocol)
    }
}

impl From<secure::SecureProtocol<MultiKeyVdpf>> for ProtocolWrapper {
    fn from(protocol: secure::SecureProtocol<MultiKeyVdpf>) -> Self {
        Self::SecureMultiKey(protocol)
    }
}

impl ProtocolWrapper {
    pub fn new(
        security_bytes: Option<u32>,
        multi_key: bool,
        groups: usize,
        channels: usize,
        msg_size: usize,
    ) -> Self {
        match security_bytes {
            Some(security_bytes) => {
                if multi_key {
                    secure::SecureProtocol::with_group_prg_dpf(
                        security_bytes,
                        channels,
                        groups,
                        msg_size,
                    )
                    .into()
                } else {
                    assert_eq!(groups, 2);
                    secure::SecureProtocol::with_aes_prg_dpf(security_bytes, channels, msg_size)
                        .into()
                }
            }
            None => insecure::InsecureProtocol::new(groups, channels, msg_size).into(),
        }
    }

    pub fn num_parties(&self) -> usize {
        match self {
            Self::Secure(protocol) => protocol.num_parties(),
            Self::SecureMultiKey(protocol) => protocol.num_parties(),
            Self::Insecure(protocol) => protocol.num_parties(),
        }
    }

    pub fn num_channels(&self) -> usize {
        match self {
            Self::Secure(protocol) => protocol.num_channels(),
            Self::SecureMultiKey(protocol) => protocol.num_channels(),
            Self::Insecure(protocol) => protocol.num_channels(),
        }
    }

    pub fn message_len(&self) -> usize {
        match self {
            Self::Secure(protocol) => protocol.message_len(),
            Self::SecureMultiKey(protocol) => protocol.message_len(),
            Self::Insecure(protocol) => protocol.message_len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::convert::TryInto;

    proptest! {
        #[test]
        fn test_secure_channel_key_proto_roundtrips(key in any::<secure::ChannelKey<BasicVdpf>>()) {
            let wrapped: ChannelKeyWrapper = key.clone().into();
            assert_eq!(key, wrapped.try_into().unwrap());
        }
    }
}
