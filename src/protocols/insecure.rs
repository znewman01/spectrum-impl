use crate::proto::{self, AuditShare, WriteToken};
use crate::protocols::{Accumulatable, Protocol};

#[derive(Debug, Clone, PartialEq)]
pub struct InsecureChannelKey(usize, String);

#[derive(Debug, Clone)]
pub struct InsecureWriteToken(Option<(u8, InsecureChannelKey)>);

impl From<WriteToken> for InsecureWriteToken {
    fn from(token: WriteToken) -> Self {
        let insecure_token_proto = token.insecure_token.unwrap();
        if let Some(data) = insecure_token_proto.data {
            assert_eq!(data.len(), 1);
            InsecureWriteToken(Some(
                data[0],
                InsecureChannelKey(
                    insecure_token_proto.channel_idx.unwrap(),
                    insecure_token_proto.key.unwrap(),
                ),
            ))
        } else {
            InsecureWriteToken(None)
        }
    }
}

impl Into<WriteToken> for InsecureWriteToken {
    fn into(self) -> WriteToken {
        WriteToken {
            insecure_token: match self.0 {
                Some(data, key) => Some(proto::InsecureWriteToken {
                    data: Some(vec![data]),
                    channel_idx: Some(key.0),
                    key: Some(key.1),
                }),
                _ => None,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct InsecureAuditShare(bool);

impl From<AuditShare> for InsecureAuditShare {
    fn from(share: AuditShare) -> Self {
        let insecure_share_proto = share.insecure_audit_share.unwrap();
        InsecureAuditShare(insecure_share_proto.okay().unwrap())
    }
}

// impl Into<AuditShare> for InsecureAuditShare {
//     fn into(self) -> WriteToken {
//         match self.0 {
//             Some(data, key) => WriteToken {
//                 data: Some(vec![data]),
//                 channel_idx: Some(key.0),
//                 key: Some(key.1),
//             },
//             _ => WriteToken::default(),
//         }
//     }
// }

impl InsecureWriteToken {
    fn new(data: u8, key: InsecureChannelKey) -> Self {
        InsecureWriteToken(Some((data, key)))
    }

    fn empty() -> Self {
        InsecureWriteToken(None)
    }
}

impl Accumulatable for u8 {
    fn accumulate(&mut self, rhs: Self) {
        *self ^= rhs;
    }

    fn new(size: usize) -> Self {
        assert_eq!(size, 1);
        Default::default()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct InsecureProtocol {
    parties: usize,
    channels: usize,
}

impl InsecureProtocol {
    pub fn new(parties: usize, channels: usize) -> InsecureProtocol {
        InsecureProtocol { parties, channels }
    }
}

impl Protocol for InsecureProtocol {
    type Message = u8;
    type ChannelKey = InsecureChannelKey; // channel number, password
    type WriteToken = InsecureWriteToken; // message, index, maybe a key
    type AuditShare = InsecureAuditShare;
    type Accumulator = Vec<u8>;

    fn num_parties(&self) -> usize {
        self.parties
    }

    fn num_channels(&self) -> usize {
        self.channels
    }

    fn broadcast(&self, message: u8, key: InsecureChannelKey) -> Vec<InsecureWriteToken> {
        let mut data = vec![InsecureWriteToken::empty(); self.parties - 1];
        data.push(InsecureWriteToken::new(message, key));
        data
    }

    fn null_broadcast(&self) -> Vec<InsecureWriteToken> {
        vec![InsecureWriteToken::empty(); self.parties]
    }

    fn gen_audit(
        &self,
        keys: &[InsecureChannelKey],
        token: InsecureWriteToken,
    ) -> Vec<InsecureAuditShare> {
        let audit_share = match token {
            InsecureWriteToken(Some((_, key))) => {
                let InsecureChannelKey(idx, _) = key;
                match keys.get(idx) {
                    Some(expected_key) => InsecureAuditShare(&key == expected_key),
                    None => InsecureAuditShare(false),
                }
            }
            _ => true,
        };
        vec![audit_share; self.parties]
    }

    fn check_audit(&self, tokens: Vec<InsecureAuditShare>) -> bool {
        assert_eq!(tokens.len(), self.parties);
        tokens.into_iter().all(|x| x.0)
    }

    fn to_accumulator(&self, token: InsecureWriteToken) -> Vec<u8> {
        let mut accumulator = vec![u8::default(); self.num_channels() - 1];
        if let InsecureWriteToken(Some((data, key))) = token {
            let InsecureChannelKey(idx, _) = key;
            accumulator.insert(idx, data);
        } else {
            accumulator.push(u8::default());
        }
        accumulator
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const CHANNELS: usize = 3;

    fn protocols(channels: usize) -> impl Strategy<Value = InsecureProtocol> {
        (2usize..100usize).prop_map(move |p| InsecureProtocol::new(p, channels))
    }

    fn keys(channels: usize) -> impl Strategy<Value = Vec<InsecureChannelKey>> {
        Just(
            (0..channels)
                .map(|idx| InsecureChannelKey(idx, format!("password{}", idx)))
                .collect(),
        )
    }

    fn bad_keys(channels: usize) -> impl Strategy<Value = InsecureChannelKey> {
        (0..channels)
            .prop_flat_map(|idx| {
                (
                    Just(idx),
                    "\\PC+".prop_filter("Must not be the actual key!", |s| s != "password"),
                )
            })
            .prop_map(|(idx, password)| InsecureChannelKey(idx, password))
    }

    fn messages() -> impl Strategy<Value = u8> {
        any::<u8>()
    }

    fn and_accumulators(
        protocol: InsecureProtocol,
    ) -> impl Strategy<Value = (InsecureProtocol, Vec<u8>)> {
        (
            Just(protocol),
            proptest::collection::vec(any::<u8>(), protocol.num_channels()),
        )
    }

    fn get_server_shares(
        protocol: InsecureProtocol,
        tokens: Vec<InsecureWriteToken>,
        keys: Vec<InsecureChannelKey>,
    ) -> Vec<Vec<bool>> {
        let mut server_shares: Vec<Vec<bool>> = vec![Vec::new(); protocol.num_parties()];
        for token in tokens {
            for (idx, share) in protocol.gen_audit(&keys, token).into_iter().enumerate() {
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
            msg in messages().prop_filter("Broadcasting null message okay!", |m| *m != u8::default()),
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
            let before_msgs: Vec<u8> = accumulator.clone().into();
            assert_eq!(before_msgs.len(), protocol.num_channels());

            for write_token in protocol.null_broadcast() {
                accumulator.accumulate(protocol.to_accumulator(write_token));
            }
            let after_msgs: Vec<u8> = accumulator.into();

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
            for write_token in protocol.broadcast(msg, good_key) {
                accumulator.accumulate(protocol.to_accumulator(write_token));
            }
            let recovered_msgs: Vec<u8> = accumulator.into();

            assert_eq!(recovered_msgs.len(), protocol.num_channels());
            for (msg_idx, actual_msg) in recovered_msgs.into_iter().enumerate() {
                if msg_idx == idx {
                    assert_eq!(actual_msg, msg);
                } else {
                    assert_eq!(actual_msg, u8::default())
                }
            }
        }
    }
}
