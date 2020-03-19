use crate::proto;
use crate::protocols::{Bytes, ChannelKeyWrapper, Protocol};

use std::convert::TryFrom;
use std::convert::TryInto;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChannelKey(usize, String);

impl ChannelKey {
    pub fn new(idx: usize, password: String) -> Self {
        ChannelKey(idx, password)
    }
}

impl TryFrom<ChannelKeyWrapper> for ChannelKey {
    type Error = &'static str;

    fn try_from(wrapper: ChannelKeyWrapper) -> Result<Self, Self::Error> {
        #![allow(irrefutable_let_patterns)] // until we introduce multiple protocols
        if let ChannelKeyWrapper::Insecure(idx, password) = wrapper {
            Ok(ChannelKey::new(idx, password))
        } else {
            Err("Invalid channel key")
        }
    }
}

impl Into<ChannelKeyWrapper> for ChannelKey {
    fn into(self) -> ChannelKeyWrapper {
        ChannelKeyWrapper::Insecure(self.0, self.1)
    }
}

#[derive(Debug, Clone)]
pub struct WriteToken(Option<(Bytes, ChannelKey)>);

impl From<proto::WriteToken> for WriteToken {
    fn from(token: proto::WriteToken) -> Self {
        #![allow(irrefutable_let_patterns)] // until we introduce multiple protocols
        if let proto::write_token::Token::InsecureToken(insecure_token_proto) = token.token.unwrap()
        {
            let data = insecure_token_proto.data;
            if !data.is_empty() {
                WriteToken(Some((
                    data.into(),
                    ChannelKey(
                        insecure_token_proto.channel_idx.try_into().unwrap(),
                        insecure_token_proto.key,
                    ),
                )))
            } else {
                WriteToken(None)
            }
        } else {
            panic!();
        }
    }
}

impl Into<proto::WriteToken> for WriteToken {
    fn into(self) -> proto::WriteToken {
        proto::WriteToken {
            token: Some(proto::write_token::Token::InsecureToken(match self.0 {
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

#[derive(Debug, Clone)]
pub struct AuditShare(bool);

impl From<proto::AuditShare> for AuditShare {
    fn from(share: proto::AuditShare) -> Self {
        #![allow(irrefutable_let_patterns)] // until we introduce multiple protocols
        if let proto::audit_share::AuditShare::InsecureAuditShare(insecure_share_proto) =
            share.audit_share.unwrap()
        {
            AuditShare(insecure_share_proto.okay)
        } else {
            panic!();
        }
    }
}

impl Into<proto::AuditShare> for AuditShare {
    fn into(self) -> proto::AuditShare {
        proto::AuditShare {
            audit_share: Some(proto::audit_share::AuditShare::InsecureAuditShare(
                proto::InsecureAuditShare { okay: self.0 },
            )),
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

#[derive(Debug, Clone, Copy)]
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

#[cfg(test)]
mod tests {
    #![allow(clippy::identity_conversion)]
    use super::*;
    use crate::protocols::accumulator::Accumulatable;
    use proptest::prelude::*;

    const CHANNELS: usize = 3;
    const MSG_LEN: usize = 10;

    fn protocols(channels: usize) -> impl Strategy<Value = InsecureProtocol> {
        (2usize..100usize).prop_map(move |p| InsecureProtocol::new(p, channels, MSG_LEN))
    }

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

    fn messages() -> impl Strategy<Value = Bytes> {
        proptest::collection::vec(any::<u8>(), MSG_LEN).prop_map(Into::into)
    }

    fn and_accumulators(
        protocol: InsecureProtocol,
    ) -> impl Strategy<Value = (InsecureProtocol, Vec<Bytes>)> {
        (
            Just(protocol),
            proptest::collection::vec(messages(), protocol.num_channels()),
        )
    }

    fn get_server_shares(
        protocol: InsecureProtocol,
        tokens: Vec<WriteToken>,
        keys: Vec<ChannelKey>,
    ) -> Vec<Vec<AuditShare>> {
        let mut server_shares = vec![Vec::new(); protocol.num_parties()];
        for token in tokens {
            for (idx, share) in protocol.gen_audit(&keys, &token).into_iter().enumerate() {
                server_shares[idx].push(share);
            }
        }
        server_shares
    }

    proptest! {
        #[test]
        fn test_null_broadcast_passes_audit(
            protocol in protocols(CHANNELS),
            keys in keys(CHANNELS)
        ) {
            let tokens = protocol.null_broadcast();
            prop_assert_eq!(tokens.len(), protocol.num_parties());

            for shares in get_server_shares(protocol, tokens, keys) {
                prop_assert!(protocol.check_audit(shares));
            }
        }

        #[test]
        fn test_broadcast_passes_audit(
            protocol in protocols(CHANNELS),
            msg in messages(),
            keys in keys(CHANNELS),
            idx in any::<prop::sample::Index>(),
        ) {
            let good_key = keys[idx.index(keys.len())].clone();
            let tokens = protocol.broadcast(msg, good_key);
            prop_assert_eq!(tokens.len(), protocol.num_parties());

            for shares in get_server_shares(protocol, tokens, keys) {
                prop_assert!(protocol.check_audit(shares));
            }
        }

        #[test]
        fn test_broadcast_bad_key_fails_audit(
            protocol in protocols(CHANNELS),
            msg in messages().prop_filter("Broadcasting null message okay!", |m| *m != Bytes::default()),
            good_key in keys(CHANNELS),
            bad_key in bad_keys(CHANNELS)
        ) {
            let tokens = protocol.broadcast(msg, bad_key);
            prop_assert_eq!(tokens.len(), protocol.num_parties());

            for shares in get_server_shares(protocol, tokens, good_key) {
                prop_assert!(!protocol.check_audit(shares));
            }
        }

        #[test]
        fn test_null_broadcast_messages_unchanged(
            (protocol, mut accumulator) in protocols(CHANNELS).prop_flat_map(and_accumulators)
        ) {
            let before_msgs: Vec<Bytes> = accumulator.clone().into();
            assert_eq!(before_msgs.len(), protocol.num_channels());

            for write_token in protocol.null_broadcast() {
                accumulator.combine(protocol.to_accumulator(write_token));
            }
            let after_msgs: Vec<Bytes> = accumulator.into();

            assert_eq!(before_msgs, after_msgs);
        }

        #[test]
        fn test_broadcast_recovers_message(
            protocol in protocols(CHANNELS),
            msg in messages(),
            keys in keys(CHANNELS),
            idx in any::<prop::sample::Index>(),
        ) {
            let idx = idx.index(keys.len());
            let good_key = keys[idx].clone();

            let mut accumulator = protocol.new_accumulator();
            for write_token in protocol.broadcast(msg.clone(), good_key) {
                accumulator.combine(protocol.to_accumulator(write_token));
            }
            let recovered_msgs: Vec<Bytes> = accumulator.into();

            assert_eq!(recovered_msgs.len(), protocol.num_channels());
            for (msg_idx, actual_msg) in recovered_msgs.into_iter().enumerate() {
                if msg_idx == idx {
                    assert_eq!(actual_msg, msg);
                } else {
                    assert_eq!(actual_msg, Bytes::empty(MSG_LEN))
                }
            }
        }
    }
}
