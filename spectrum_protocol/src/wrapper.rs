// https://github.com/rust-lang/rust-clippy/issues/6594
#![allow(clippy::unit_arg)]
use crate::{
    insecure,
    secure::{self},
    Protocol,
};

use serde::{Deserialize, Serialize};
use spectrum_primitives::{AuthKey, MultiKeyVdpf, TwoKeyPubAuthKey, TwoKeyPubVdpf, TwoKeyVdpf};

use std::convert::TryFrom;
use std::fmt::Debug;

type SecureProtocolTwoKey = secure::Wrapper<TwoKeyVdpf>;
type SecureProtocolTwoKeyPub = secure::Wrapper<TwoKeyPubVdpf>;
type SecureProtocolMultiKey = secure::Wrapper<MultiKeyVdpf>;

#[cfg(any(test, feature = "testing"))]
use proptest_derive::Arbitrary;

#[cfg_attr(any(test, feature = "testing"), derive(Arbitrary))]
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum ChannelKeyWrapper {
    Insecure(String),
    Secure(AuthKey),
    SecurePub(TwoKeyPubAuthKey),
}

impl TryFrom<ChannelKeyWrapper> for TwoKeyPubAuthKey {
    type Error = &'static str;

    fn try_from(wrapper: ChannelKeyWrapper) -> Result<Self, Self::Error> {
        if let ChannelKeyWrapper::SecurePub(secret) = wrapper {
            Ok(secret)
        } else {
            Err("Invalid channel key")
        }
    }
}

impl From<TwoKeyPubAuthKey> for ChannelKeyWrapper {
    fn from(key: TwoKeyPubAuthKey) -> Self {
        ChannelKeyWrapper::SecurePub(key)
    }
}

impl TryFrom<ChannelKeyWrapper> for AuthKey {
    type Error = &'static str;

    fn try_from(wrapper: ChannelKeyWrapper) -> Result<Self, Self::Error> {
        if let ChannelKeyWrapper::Secure(secret) = wrapper {
            Ok(secret)
        } else {
            Err("Invalid channel key")
        }
    }
}

impl From<AuthKey> for ChannelKeyWrapper {
    fn from(key: AuthKey) -> Self {
        ChannelKeyWrapper::Secure(key)
    }
}

impl TryFrom<ChannelKeyWrapper> for String {
    type Error = &'static str;

    fn try_from(wrapper: ChannelKeyWrapper) -> Result<Self, Self::Error> {
        if let ChannelKeyWrapper::Insecure(password) = wrapper {
            Ok(password)
        } else {
            Err("Invalid channel key")
        }
    }
}

impl From<String> for ChannelKeyWrapper {
    fn from(value: String) -> ChannelKeyWrapper {
        ChannelKeyWrapper::Insecure(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProtocolWrapper {
    Secure(SecureProtocolTwoKey),
    SecurePub(SecureProtocolTwoKeyPub),
    Insecure(insecure::InsecureProtocol),
    SecureMultiKey(SecureProtocolMultiKey),
}

impl From<insecure::InsecureProtocol> for ProtocolWrapper {
    fn from(protocol: insecure::InsecureProtocol) -> Self {
        Self::Insecure(protocol)
    }
}

impl From<SecureProtocolTwoKey> for ProtocolWrapper {
    fn from(protocol: SecureProtocolTwoKey) -> Self {
        Self::Secure(protocol)
    }
}

impl From<SecureProtocolTwoKeyPub> for ProtocolWrapper {
    fn from(protocol: SecureProtocolTwoKeyPub) -> Self {
        Self::SecurePub(protocol)
    }
}

impl From<SecureProtocolMultiKey> for ProtocolWrapper {
    fn from(protocol: SecureProtocolMultiKey) -> Self {
        Self::SecureMultiKey(protocol)
    }
}

impl ProtocolWrapper {
    pub fn new(
        security_bytes: bool,
        multi_key: bool,
        groups: usize,
        channels: usize,
        msg_size: usize,
    ) -> Self {
        match security_bytes {
            true => {
                if multi_key {
                    Into::<secure::Wrapper<_>>::into(MultiKeyVdpf::with_channels_parties_msg_size(
                        channels, groups, msg_size,
                    ))
                    .into()
                } else {
                    assert_eq!(groups, 2);
                    Into::<secure::Wrapper<_>>::into(TwoKeyVdpf::with_channels_msg_size(
                        channels, msg_size,
                    ))
                    .into()
                }
            }
            false => insecure::InsecureProtocol::new(groups, channels, msg_size).into(),
        }
    }

    pub fn num_parties(&self) -> usize {
        match self {
            Self::Secure(protocol) => protocol.num_parties(),
            Self::SecurePub(protocol) => protocol.num_parties(),
            Self::SecureMultiKey(protocol) => protocol.num_parties(),
            Self::Insecure(protocol) => protocol.num_parties(),
        }
    }

    pub fn num_channels(&self) -> usize {
        match self {
            Self::Secure(protocol) => protocol.num_channels(),
            Self::SecurePub(protocol) => protocol.num_channels(),
            Self::SecureMultiKey(protocol) => protocol.num_channels(),
            Self::Insecure(protocol) => protocol.num_channels(),
        }
    }

    pub fn message_len(&self) -> usize {
        match self {
            Self::Secure(protocol) => protocol.message_len(),
            Self::SecurePub(protocol) => protocol.message_len(),
            Self::SecureMultiKey(protocol) => protocol.message_len(),
            Self::Insecure(protocol) => protocol.message_len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use spectrum_primitives::check_roundtrip;
    use std::convert::TryInto;

    check_roundtrip!(
        String,
        Into::<ChannelKeyWrapper>::into,
        |w: ChannelKeyWrapper| w.try_into().unwrap(),
        string_channelkeywrapper_rt
    );

    check_roundtrip!(
        AuthKey,
        Into::<ChannelKeyWrapper>::into,
        |w: ChannelKeyWrapper| w.try_into().unwrap(),
        authkey_channelkeywrapper_rt
    );
}
