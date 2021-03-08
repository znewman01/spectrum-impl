use crate::Accumulatable;

pub trait Protocol {
    type ChannelKey;
    type WriteToken;
    type AuditShare;
    type Accumulator: Accumulatable;

    // General protocol properties
    fn num_parties(&self) -> usize;
    fn num_channels(&self) -> usize;
    fn message_len(&self) -> usize;

    // Client algorithms
    fn broadcast(
        &self,
        message: Self::Accumulator,
        idx: usize,
        key: Self::ChannelKey,
    ) -> Vec<Self::WriteToken>;
    fn cover(&self) -> Vec<Self::WriteToken>;

    // Server algorithms
    fn gen_audit(
        &self,
        keys: &[Self::ChannelKey],
        token: Self::WriteToken,
    ) -> Vec<Self::AuditShare>;
    fn check_audit(&self, tokens: Vec<Self::AuditShare>) -> bool;

    fn new_accumulator(&self) -> Vec<Self::Accumulator>;

    fn to_accumulator(&self, token: Self::WriteToken) -> Vec<Self::Accumulator>;
}

/// Checks correctness of protocol implementation.
///
/// Also, if feature `proto` is enable, tests roundtrips to/from proto format.
#[cfg(test)]
macro_rules! check_protocol {
    ($type:ty) => {
        mod protocol {
            #![allow(unused_imports)]
            use super::*;
            use crate::Accumulatable;
            use crate::Protocol;
            use proptest::prelude::*;

            /// Returns a vector of (the vector of audit tokens that each server receives) in the protocol.
            ///
            /// That is, both the outer and inner vectors have length `protocol.num_parties()`.
            fn get_server_shares(
                protocol: &$type,
                tokens: Vec<<$type as Protocol>::WriteToken>,
                keys: Vec<<$type as Protocol>::ChannelKey>,
            ) -> Vec<Vec<<$type as Protocol>::AuditShare>>
            {
                let mut server_shares = vec![Vec::new(); protocol.num_parties()];
                for token in tokens {
                    for (idx, share) in protocol.gen_audit(&keys, token).into_iter().enumerate() {
                        server_shares[idx].push(share);
                    }
                }
                server_shares
            }

            fn protocol_with_keys() -> impl Strategy<Value=($type, Vec<<$type as Protocol>::ChannelKey>)> {
                use proptest::collection::vec;
                any::<$type>().prop_flat_map(|protocol| {
                    (
                        Just(protocol.clone()),
                        vec(any::<<$type as Protocol>::ChannelKey>(), protocol.num_channels()),
                    )
                })
            }

            fn protocol_with_keys_msg() -> impl Strategy<Value=($type, Vec<<$type as Protocol>::ChannelKey>, <$type as Protocol>::Accumulator)> {
                use proptest::collection::vec;
                protocol_with_keys().prop_flat_map(|(protocol, keys)| {
                    let length = protocol.message_len();
                    (Just(protocol), Just(keys), any_with::<<$type as Protocol>::Accumulator>(length.into()))
                })
            }

            fn protocol_with_keys_msg_bad_key() -> impl Strategy<Value=($type, Vec<<$type as Protocol>::ChannelKey>, <$type as Protocol>::Accumulator, <$type as Protocol>::ChannelKey)> {
                use prop::sample::Index;
                (protocol_with_keys_msg(), any::<Index>()).prop_flat_map(|((protocol, good_keys, msg), idx)| {
                    let idx = idx.index(protocol.num_channels());
                    let good_key = good_keys[idx].clone();
                    let bad_key = any::<<$type as Protocol>::ChannelKey>()
                        .prop_filter("must be different", move |key| key != &good_key);
                    (Just(protocol), Just(good_keys), Just(msg), bad_key)
                })
            }

            fn protocol_with_accumulator() -> impl Strategy<Value=($type, Vec<<$type as Protocol>::Accumulator>)>{
                use prop::collection::vec;
                any::<$type>().prop_flat_map(move |protocol| {
                    let channels = protocol.num_channels();
                    let empty = protocol.new_accumulator();
                    let params = empty[0].params();
                    (Just(protocol), vec(any_with::<<$type as Protocol>::Accumulator>(params.into()), channels))
                })
            }

            proptest! {
                #[test]
                fn test_cover_complete((protocol, keys) in protocol_with_keys()) {
                    let tokens = protocol.cover();
                    prop_assert_eq!(tokens.len(), protocol.num_parties(), "cover should give one message per party");

                    for shares in get_server_shares(&protocol, tokens, keys) {
                        prop_assert!(protocol.check_audit(shares), "audit should pass");
                    }
                }

                #[test]
                fn test_broadcast_complete(
                    (protocol, keys, msg) in protocol_with_keys_msg(),
                    idx: prop::sample::Index,
                )
                {
                    let idx = idx.index(keys.len());
                    let good_key = keys[idx].clone();
                    let tokens = protocol.broadcast(msg, idx, good_key);
                    prop_assert_eq!(tokens.len(), protocol.num_parties(), "broadcast should give one message per party");

                    for shares in get_server_shares(&protocol, tokens, keys) {
                        prop_assert!(protocol.check_audit(shares), "audit should pass");
                    }
                }

                #[test]
                fn test_broadcast_soundness(
                    (protocol, good_keys, msg, bad_key) in protocol_with_keys_msg_bad_key(),
                    idx: prop::sample::Index,
                )
                {
                    let idx = idx.index(good_keys.len());
                    prop_assume!(!good_keys.contains(&bad_key));
                    let tokens = protocol.broadcast(msg, idx, bad_key);
                    prop_assert_eq!(tokens.len(), protocol.num_parties());

                    for shares in get_server_shares(&protocol, tokens, good_keys) {
                        prop_assert!(!protocol.check_audit(shares), "audit should fail");
                    }
                }

                /// Tests that cover messages do not change the accumulator value.
                #[test]
                fn test_cover_correct((protocol, mut accumulator) in protocol_with_accumulator()) {
                    let expected = accumulator.clone();
                    prop_assert_eq!(accumulator.len(), protocol.num_channels());
                    for write_token in protocol.cover() {
                        accumulator.combine(protocol.to_accumulator(write_token));
                    }
                    prop_assert_eq!(accumulator, expected);
                }

                #[test]
                fn test_broadcast_correct(
                    (protocol, keys, msg) in protocol_with_keys_msg(),
                    key_idx: prop::sample::Index,
                )
                {
                    let key_idx = key_idx.index(keys.len());
                    let good_key = keys[key_idx].clone();

                    let mut accumulator = protocol.new_accumulator();
                    for write_token in protocol.broadcast(msg.clone(), key_idx, good_key) {
                        accumulator.combine(protocol.to_accumulator(write_token));
                    }

                    let recovered_msgs = accumulator;
                    prop_assert_eq!(recovered_msgs.len(), protocol.num_channels(), "wrong accumulator size");
                    for (msg_idx, actual_msg) in recovered_msgs.into_iter().enumerate() {
                        if msg_idx == key_idx {
                            prop_assert_eq!(
                                actual_msg.into(): <$type as Protocol>::Accumulator,
                                msg.clone(),
                                "Channel was incorrect"
                            );
                        } else {
                            prop_assert_eq!(
                                actual_msg.into(): <$type as Protocol>::Accumulator,
                                <$type as Protocol>::Accumulator::empty(protocol.message_len().into()),
                                "Channel was non-null"
                            )
                        }
                    }
                }
            }
            #[cfg(feature = "proto")]
            mod proto {
                use super::*;
                use crate::proto::{AuditShare, Share, WriteToken};
                use std::convert::TryFrom;
                use crate::Protocol;
                use spectrum_primitives::check_roundtrip;
                check_roundtrip!(
                    <$type as Protocol>::WriteToken,
                    WriteToken::from,
                    |p| <$type as Protocol>::WriteToken::try_from(p).unwrap(),
                    write_token_rt
                );

                check_roundtrip!(
                    <$type as Protocol>::AuditShare,
                    AuditShare::from,
                    |p| <$type as Protocol>::AuditShare::try_from(p).unwrap(),
                    audit_share_rt
                );

                check_roundtrip!(
                    Vec::<<$type as Protocol>::Accumulator>,
                    Share::from,
                    |p| Vec::<<$type as Protocol>::Accumulator>::try_from(p).unwrap(),
                    share_rt
                );
            }
        }
    };
}
