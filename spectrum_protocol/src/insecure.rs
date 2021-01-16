use crate::Protocol;
use spectrum_primitives::bytes::Bytes;

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

    fn null_broadcast(&self) -> Vec<WriteToken> {
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

/// Test helpers
#[cfg(any(test, feature = "testing"))]
mod testing {
    use super::*;
    use crate::tests::*;

    pub fn protocols(channels: usize) -> impl Strategy<Value = InsecureProtocol> {
        (2..100usize).prop_map(move |p| InsecureProtocol::new(p, channels, MSG_LEN))
    }
}

#[cfg(any(test, feature = "testing"))]
impl Arbitrary for InsecureProtocol {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use testing::*;
        (1..10usize).prop_flat_map(protocols).boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::tests::*;

    fn keys(channels: usize) -> impl Strategy<Value = Vec<ChannelKey>> {
        Just(
            (0..channels)
                .map(|idx| ChannelKey(idx, format!("password{}", idx)))
                .collect(),
        )
    }

    fn bad_keys(channels: usize) -> impl Strategy<Value = ChannelKey> {
        (0..channels)
            .prop_flat_map(|idx| {
                (
                    Just(idx),
                    "\\PC+".prop_filter("Must not be the actual key!", |s| s != "password"),
                )
            })
            .prop_map(|(idx, password)| ChannelKey(idx, password))
    }

    proptest! {
        #[test]
        fn test_null_broadcast_passes_audit(
            protocol in protocols(CHANNELS),
            keys in keys(CHANNELS)
        ) {
            check_null_broadcast_passes_audit(protocol, keys);
        }

        #[test]
        fn test_broadcast_passes_audit(
            protocol in protocols(CHANNELS),
            msg in messages(),
            keys in keys(CHANNELS),
            idx in any::<prop::sample::Index>(),
        ) {
            let idx = idx.index(keys.len());
            check_broadcast_passes_audit(protocol, msg, keys, idx);
        }

        #[test]
        fn test_broadcast_bad_key_fails_audit(
            protocol in protocols(CHANNELS),
            msg in messages().prop_filter("Broadcasting null message okay!", |m| *m != Bytes::empty(MSG_LEN)),
            good_keys in keys(CHANNELS),
            bad_key in bad_keys(CHANNELS)
        ) {
            check_broadcast_bad_key_fails_audit(protocol, msg, good_keys, bad_key);
        }

        #[test]
        fn test_null_broadcast_messages_unchanged(
            (protocol, accumulator) in protocols(CHANNELS).prop_flat_map(and_accumulators)
        ) {
            check_null_broadcast_messages_unchanged(protocol, accumulator);
        }

        #[test]
        fn test_broadcast_recovers_message(
            protocol in protocols(CHANNELS),
            msg in messages(),
            keys in keys(CHANNELS),
            idx in any::<prop::sample::Index>(),
        ) {
            let idx = idx.index(keys.len());
            check_broadcast_recovers_message(protocol, msg, keys, idx);
        }
    }
}
