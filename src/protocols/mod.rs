#![allow(dead_code)]
pub mod accumulator;
pub mod table;
// TODO(zjn): adapter to/from protos

trait Protocol {
    type Message;
    type ChannelKey;
    type WriteToken;
    type AuditShare;

    fn num_parties(&self) -> usize;
    fn num_channels(&self) -> usize;

    // Client algorithms
    fn broadcast(&self, message: Self::Message, key: Self::ChannelKey) -> Vec<Self::WriteToken>;
    fn null_broadcast(&self) -> Vec<Self::WriteToken>;

    // combine: WriteToken must be aggregatable
    fn gen_audit(
        &self,
        keys: &Vec<Self::ChannelKey>,
        token: Self::WriteToken,
    ) -> Vec<Self::AuditShare>;
    fn check_audit(&self, tokens: Vec<Self::AuditShare>) -> bool;
}

#[derive(Debug, Clone, Copy)]
struct InsecureProtocol {
    parties: usize,
    channels: usize,
}

impl InsecureProtocol {
    fn new(parties: usize, channels: usize) -> InsecureProtocol {
        InsecureProtocol { parties, channels }
    }
}

impl Protocol for InsecureProtocol {
    type Message = u8;
    type ChannelKey = (usize, String); // channel number, password
    type WriteToken = Option<(Self::Message, Self::ChannelKey)>; // message, index, maybe a key
    type AuditShare = bool;

    fn num_parties(&self) -> usize {
        self.parties
    }

    fn num_channels(&self) -> usize {
        self.channels
    }

    fn broadcast(
        &self,
        message: Self::Message,
        key: Self::ChannelKey,
    ) -> Vec<Option<(Self::Message, Self::ChannelKey)>> {
        let mut data = vec![None; self.parties - 1];
        data.push(Some((message, key)));
        data
    }

    fn null_broadcast(&self) -> Vec<Option<(Self::Message, Self::ChannelKey)>> {
        vec![None; self.parties]
    }

    fn gen_audit(
        &self,
        keys: &Vec<(usize, String)>,
        token: Option<(Self::Message, (usize, String))>,
    ) -> Vec<bool> {
        let proof_ok = match token {
            None => true,
            Some((_, (idx, password))) => (idx, password) == keys[idx],
        };
        vec![proof_ok; self.parties]
    }

    fn check_audit(&self, tokens: Vec<bool>) -> bool {
        assert_eq!(tokens.len(), self.parties);
        tokens.into_iter().all(|x| x)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unit_arg)] // we *want* unit values for channel keys
    use super::*;
    use proptest::prelude::*;

    const CHANNELS: usize = 3;

    fn protocols(channels: usize) -> impl Strategy<Value = InsecureProtocol> {
        (2usize..100usize).prop_map(move |p| InsecureProtocol::new(p, channels))
    }

    fn keys(channels: usize) -> impl Strategy<Value = Vec<(usize, String)>> {
        Just(
            (0..channels)
                .map(|idx| (idx, format!("password{}", idx)))
                .collect(),
        )
    }

    fn bad_keys(channels: usize) -> impl Strategy<Value = (usize, String)> {
        (0..channels).prop_flat_map(|idx| {
            (
                Just(idx),
                "\\PC+".prop_filter("Must not be the actual key!", |s| s != "password"),
            )
        })
    }

    fn messages() -> impl Strategy<Value = u8> {
        any::<u8>()
    }

    fn get_server_shares(
        protocol: InsecureProtocol,
        tokens: Vec<Option<(u8, (usize, String))>>,
        keys: Vec<(usize, String)>,
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
            message in messages(),
            keys in keys(CHANNELS),
            idx in any::<prop::sample::Index>(),
        ) {
            let good_key = keys[idx.index(keys.len())].clone();
            let tokens = protocol.broadcast(message, good_key);
            prop_assert_eq!(tokens.len(), protocol.num_parties());

            for shares in get_server_shares(protocol, tokens, keys) {
                prop_assert!(protocol.check_audit(shares));
            }
        }

        #[test]
        fn test_broadcast_bad_key_fails_audit(
            protocol in protocols(CHANNELS),
            message in messages().prop_filter("Broadcasting null message okay!", |m| *m != u8::default()),
            good_key in keys(CHANNELS),
            bad_key in bad_keys(CHANNELS)
        ) {
            let tokens = protocol.broadcast(message, bad_key);
            prop_assert_eq!(tokens.len(), protocol.num_parties());

            for shares in get_server_shares(protocol, tokens, good_key) {
                prop_assert!(!protocol.check_audit(shares));
            }
        }
    }
}
