pub mod accumulator;
pub mod insecure;
pub mod secure;
pub mod wrapper;

use crate::bytes::Bytes;

type Accumulator = Vec<Bytes>;

pub trait Protocol {
    type ChannelKey;
    type WriteToken;
    type AuditShare;

    // General protocol properties
    fn num_parties(&self) -> usize;
    fn num_channels(&self) -> usize;
    fn message_len(&self) -> usize;

    // Client algorithms
    fn broadcast(&self, message: Bytes, key: Self::ChannelKey) -> Vec<Self::WriteToken>;
    fn null_broadcast(&self) -> Vec<Self::WriteToken>;

    // Server algorithms
    fn gen_audit(
        &self,
        keys: &[Self::ChannelKey],
        token: &Self::WriteToken,
    ) -> Vec<Self::AuditShare>;
    fn check_audit(&self, tokens: Vec<Self::AuditShare>) -> bool;

    fn new_accumulator(&self) -> Accumulator {
        vec![Bytes::empty(self.message_len()); self.num_channels()]
    }

    fn to_accumulator(&self, token: Self::WriteToken) -> Accumulator;
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::protocols::accumulator::Accumulatable;
    use proptest::prelude::*;
    use std::fmt::Debug;

    pub const CHANNELS: usize = 3;
    pub const MSG_LEN: usize = 64;

    pub fn messages() -> impl Strategy<Value = Bytes> {
        prop::collection::vec(any::<u8>(), MSG_LEN).prop_map(Into::into)
    }

    pub fn and_messages<P>(protocol: P) -> impl Strategy<Value = (P, Bytes)>
    where
        P: Protocol + Debug + Clone,
    {
        let size = protocol.message_len();
        (Just(protocol), any_with::<Bytes>(size.into()))
    }

    pub fn and_accumulators<P>(protocol: P) -> impl Strategy<Value = (P, Vec<Bytes>)>
    where
        P: Protocol + Debug + Clone,
    {
        let channels = protocol.num_channels();
        let size = protocol.message_len();
        (
            Just(protocol),
            prop::collection::vec(any_with::<Bytes>(size.into()), channels),
        )
    }

    pub fn get_server_shares<P: Protocol>(
        protocol: &P,
        tokens: Vec<P::WriteToken>,
        keys: Vec<P::ChannelKey>,
    ) -> Vec<Vec<P::AuditShare>>
    where
        P: Protocol,
        P::AuditShare: Clone,
    {
        let mut server_shares = vec![Vec::new(); protocol.num_parties()];
        for token in tokens {
            for (idx, share) in protocol.gen_audit(&keys, &token).into_iter().enumerate() {
                server_shares[idx].push(share);
            }
        }
        server_shares
    }

    pub fn check_null_broadcast_passes_audit<P>(protocol: P, keys: Vec<P::ChannelKey>)
    where
        P: Protocol,
        P::AuditShare: Clone,
    {
        let tokens = protocol.null_broadcast();
        assert_eq!(tokens.len(), protocol.num_parties());

        for shares in get_server_shares(&protocol, tokens, keys) {
            assert!(protocol.check_audit(shares));
        }
    }

    pub fn check_broadcast_passes_audit<P>(
        protocol: P,
        msg: Bytes,
        keys: Vec<P::ChannelKey>,
        key_idx: usize,
    ) where
        P: Protocol,
        P::AuditShare: Clone,
        P::ChannelKey: Clone,
    {
        let good_key = keys[key_idx].clone();
        let tokens = protocol.broadcast(msg, good_key);
        assert_eq!(tokens.len(), protocol.num_parties());

        for shares in get_server_shares(&protocol, tokens, keys) {
            assert!(protocol.check_audit(shares));
        }
    }

    pub fn check_broadcast_bad_key_fails_audit<P>(
        protocol: P,
        msg: Bytes,
        good_keys: Vec<P::ChannelKey>,
        bad_key: P::ChannelKey,
    ) where
        P: Protocol,
        P::AuditShare: Clone,
        P::ChannelKey: Clone,
    {
        let tokens = protocol.broadcast(msg, bad_key);
        assert_eq!(tokens.len(), protocol.num_parties());

        for shares in get_server_shares(&protocol, tokens, good_keys) {
            assert!(!protocol.check_audit(shares));
        }
    }

    pub fn check_null_broadcast_messages_unchanged<P>(protocol: P, mut accumulator: Vec<Bytes>)
    where
        P: Protocol,
    {
        let before_msgs = accumulator.clone();
        assert_eq!(before_msgs.len(), protocol.num_channels());

        for write_token in protocol.null_broadcast() {
            accumulator.combine(protocol.to_accumulator(write_token));
        }
        let after_msgs = accumulator;

        assert_eq!(before_msgs, after_msgs);
    }

    pub fn check_broadcast_recovers_message<P>(
        protocol: P,
        msg: Bytes,
        keys: Vec<P::ChannelKey>,
        key_idx: usize,
    ) where
        P: Protocol,
        P::AuditShare: Clone,
        P::ChannelKey: Clone,
    {
        let good_key = keys[key_idx].clone();

        let mut accumulator = protocol.new_accumulator();
        for write_token in protocol.broadcast(msg.clone(), good_key) {
            accumulator.combine(protocol.to_accumulator(write_token));
        }
        let recovered_msgs = accumulator;

        assert_eq!(recovered_msgs.len(), protocol.num_channels());
        for (msg_idx, actual_msg) in recovered_msgs.into_iter().enumerate() {
            if msg_idx == key_idx {
                assert_eq!(actual_msg, msg);
            } else {
                assert_eq!(actual_msg, Bytes::empty(protocol.message_len()))
            }
        }
    }
}
