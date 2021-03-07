use crate::Protocol;
use spectrum_primitives::Bytes;

use serde::{Deserialize, Serialize};

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

#[cfg(feature = "proto")]
use {crate::proto, std::convert::TryInto};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChannelKey(pub(in crate) usize, pub(in crate) String);

impl ChannelKey {
    pub fn new(idx: usize, password: String) -> Self {
        ChannelKey(idx, password)
    }
}

#[cfg(any(test, feature = "testing"))]
impl Arbitrary for ChannelKey {
    type Parameters = (usize, Option<InsecureProtocol>);
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((idx, _protocol): Self::Parameters) -> Self::Strategy {
        any::<String>()
            .prop_map(move |password| ChannelKey::new(idx, password))
            .boxed()
    }
}

#[derive(Debug, Clone)]
pub struct WriteToken(Option<(Bytes, ChannelKey)>);

#[cfg(feature = "proto")]
impl From<proto::WriteToken> for WriteToken {
    fn from(token: proto::WriteToken) -> Self {
        if let proto::write_token::Inner::Insecure(inner) = token.inner.unwrap() {
            let data = inner.data;
            if !data.is_empty() {
                WriteToken(Some((
                    data.into(),
                    ChannelKey(inner.channel_idx.try_into().unwrap(), inner.key),
                )))
            } else {
                WriteToken(None)
            }
        } else {
            panic!();
        }
    }
}

#[cfg(feature = "proto")]
impl Into<proto::WriteToken> for WriteToken {
    fn into(self) -> proto::WriteToken {
        proto::WriteToken {
            inner: Some(proto::write_token::Inner::Insecure(match self.0 {
                Some((data, key)) => proto::InsecureWriteToken {
                    data: data.into(),
                    channel_idx: key.0 as u32,
                    key: key.1,
                },
                None => proto::InsecureWriteToken::default(),
            })),
        }
    }
}

impl WriteToken {
    fn new(data: Bytes, key: ChannelKey) -> Self {
        WriteToken(Some((data, key)))
    }

    fn empty() -> Self {
        WriteToken(None)
    }
}

#[derive(Debug, Clone)]
pub struct AuditShare(bool);

#[cfg(feature = "proto")]
impl From<proto::AuditShare> for AuditShare {
    fn from(share: proto::AuditShare) -> Self {
        if let proto::audit_share::Inner::Insecure(inner) = share.inner.unwrap() {
            AuditShare(inner.okay)
        } else {
            panic!();
        }
    }
}

#[cfg(feature = "proto")]
impl Into<proto::AuditShare> for AuditShare {
    fn into(self) -> proto::AuditShare {
        proto::AuditShare {
            inner: Some(proto::audit_share::Inner::Insecure(
                proto::InsecureAuditShare { okay: self.0 },
            )),
        }
    }
}

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
    type ChannelKey = ChannelKey; // channel number, password
    type WriteToken = WriteToken; // message, index, maybe a key
    type AuditShare = AuditShare;
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

    fn broadcast(&self, message: Bytes, key: ChannelKey) -> Vec<WriteToken> {
        let mut data = vec![WriteToken::empty(); self.parties - 1];
        data.push(WriteToken::new(message, key));
        data
    }

    fn cover(&self) -> Vec<WriteToken> {
        vec![WriteToken::empty(); self.parties]
    }

    fn gen_audit(&self, keys: &[ChannelKey], token: &WriteToken) -> Vec<AuditShare> {
        let audit_share = match token {
            WriteToken(Some((_, key))) => {
                let ChannelKey(idx, _) = key;
                match keys.get(*idx) {
                    Some(expected_key) => AuditShare(key == expected_key),
                    None => AuditShare(false),
                }
            }
            _ => AuditShare(true),
        };
        vec![audit_share; self.parties]
    }

    fn check_audit(&self, tokens: Vec<AuditShare>) -> bool {
        assert_eq!(tokens.len(), self.parties);
        tokens.into_iter().all(|x| x.0)
    }

    fn new_accumulator(&self) -> Vec<Self::Accumulator> {
        vec![Bytes::empty(self.message_len()); self.num_channels()]
    }

    fn to_accumulator(&self, token: WriteToken) -> Vec<Bytes> {
        let mut accumulator = vec![Bytes::empty(self.message_len); self.num_channels() - 1];
        if let WriteToken(Some((data, key))) = token {
            let ChannelKey(idx, _) = key;
            accumulator.insert(idx, data);
        } else {
            accumulator.push(Bytes::empty(self.message_len));
        }
        accumulator
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
check_protocol!(InsecureProtocol);
