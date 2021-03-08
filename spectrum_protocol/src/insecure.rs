// https://github.com/rust-lang/rust-clippy/issues/6594
#![allow(clippy::unit_arg)]
use crate::Protocol;
use spectrum_primitives::Bytes;

use serde::{Deserialize, Serialize};

#[cfg(any(test, feature = "testing"))]
use {proptest::prelude::*, proptest_derive::Arbitrary};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct InsecureProtocol {
    parties: usize,
    channels: usize,
    message_len: usize,
}

impl InsecureProtocol {
    pub fn new(parties: usize, channels: usize, message_len: usize) -> InsecureProtocol {
        InsecureProtocol {
            parties,
            channels,
            message_len,
        }
    }
}

impl Protocol for InsecureProtocol {
    type ChannelKey = String; // password
    type WriteToken = WriteToken; // message, index, maybe a key
    type AuditShare = bool;
    type Accumulator = Bytes;

    fn num_parties(&self) -> usize {
        self.parties
    }

    fn num_channels(&self) -> usize {
        self.channels
    }

    fn message_len(&self) -> usize {
        self.message_len
    }

    fn broadcast(&self, message: Bytes, idx: usize, key: String) -> Vec<Self::WriteToken> {
        let mut data = vec![WriteToken::empty(); self.parties - 1];
        data.push(WriteToken::new(message, idx, key));
        data
    }

    fn cover(&self) -> Vec<Self::WriteToken> {
        vec![WriteToken::empty(); self.parties]
    }

    fn gen_audit(
        &self,
        keys: &[Self::ChannelKey],
        token: Self::WriteToken,
    ) -> Vec<Self::AuditShare> {
        let audit_share = match token.inner {
            Some(token) => match keys.get(token.idx) {
                Some(expected_key) => token.key == *expected_key,
                None => false,
            },
            _ => true,
        };
        vec![audit_share; self.parties]
    }

    fn check_audit(&self, tokens: Vec<Self::AuditShare>) -> bool {
        assert_eq!(tokens.len(), self.parties);
        tokens.into_iter().all(|x| x)
    }

    fn new_accumulator(&self) -> Vec<Self::Accumulator> {
        vec![Bytes::empty(self.message_len()); self.num_channels()]
    }

    fn to_accumulator(&self, token: Self::WriteToken) -> Vec<Bytes> {
        let mut accumulator = vec![Bytes::empty(self.message_len); self.num_channels() - 1];
        if let Some(inner) = token.inner {
            accumulator.insert(inner.idx, inner.data);
        } else {
            accumulator.push(Bytes::empty(self.message_len));
        }
        accumulator
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Arbitrary))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteTokenInner {
    data: Bytes,
    idx: usize,
    key: String,
}

#[cfg_attr(any(test, feature = "testing"), derive(Arbitrary))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteToken {
    inner: Option<WriteTokenInner>,
}

impl WriteToken {
    fn new(data: Bytes, idx: usize, key: String) -> Self {
        WriteToken {
            inner: Some(WriteTokenInner { data, idx, key }),
        }
    }

    fn empty() -> Self {
        WriteToken { inner: None }
    }
}

// Boilerplate: conversions etc.

#[cfg(feature = "proto")]
use {
    crate::proto,
    std::convert::{TryFrom, TryInto},
};

#[cfg(feature = "proto")]
impl TryFrom<proto::WriteToken> for WriteToken {
    type Error = ();
    fn try_from(value: proto::WriteToken) -> Result<Self, Self::Error> {
        // WriteToken has an optional enum for the token type; this should always be populated.
        let token_enum = value.inner.ok_or(())?;
        // We expect the enum value to be an InsecureWriteToken.
        if let proto::write_token::Inner::Insecure(token) = token_enum {
            // The InsecureWriteToken has an Option<_> for the inner value.
            // If not provided, we want an empty token.
            Ok(match token.inner {
                Some(t) => {
                    let data = t.data.into();
                    let idx = t.channel_idx.try_into().map_err(|_| ())?;
                    let key = t.key;
                    WriteToken::new(data, idx, key)
                }
                None => WriteToken::empty(),
            })
        } else {
            Err(())
        }
    }
}

#[cfg(feature = "proto")]
impl From<WriteToken> for proto::WriteToken {
    fn from(value: WriteToken) -> Self {
        // The main value.
        let option = value.inner.map(|my_inner| proto::InsecureWriteTokenInner {
            data: my_inner.data.into(),
            channel_idx: my_inner.idx.try_into().unwrap(),
            key: my_inner.key,
        });
        // Stuff it in a wrapper.
        let inner = Some(proto::write_token::Inner::Insecure(
            proto::InsecureWriteToken { inner: option },
        ));
        proto::WriteToken { inner }
    }
}

#[cfg(feature = "proto")]
impl TryFrom<proto::AuditShare> for bool {
    type Error = ();

    fn try_from(value: proto::AuditShare) -> Result<Self, Self::Error> {
        let share_enum = value.inner.ok_or(())?;
        if let proto::audit_share::Inner::Insecure(share) = share_enum {
            Ok(share.okay)
        } else {
            Err(())
        }
    }
}

#[cfg(feature = "proto")]
impl From<bool> for proto::AuditShare {
    fn from(value: bool) -> proto::AuditShare {
        proto::AuditShare {
            inner: Some(proto::audit_share::Inner::Insecure(
                proto::InsecureAuditShare { okay: value },
            )),
        }
    }
}

#[cfg(feature = "proto")]
impl TryFrom<proto::Share> for Vec<Bytes> {
    type Error = ();
    fn try_from(share: proto::Share) -> Result<Self, Self::Error> {
        Ok(share.data.into_iter().map(Bytes::from).collect())
    }
}

#[cfg(feature = "proto")]
impl From<Vec<Bytes>> for proto::Share {
    fn from(value: Vec<Bytes>) -> Self {
        proto::Share {
            data: value.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(any(test, feature = "testing"))]
impl Arbitrary for InsecureProtocol {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        (2..10usize, 1..10usize, 10..50usize)
            .prop_map(|(parties, channels, len)| InsecureProtocol::new(parties, channels, len))
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    check_protocol!(InsecureProtocol);
}
