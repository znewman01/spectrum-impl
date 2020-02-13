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

    // Client algorithms
    fn broadcast(&self, message: Self::Message, key: Self::ChannelKey) -> Vec<Self::WriteToken>;
    fn null_broadcast(&self) -> Vec<Self::WriteToken>;

    // combine: WriteToken must be aggregatable
    fn gen_audit(&self, key: &Self::ChannelKey, token: Self::WriteToken) -> Vec<Self::AuditShare>;
    fn check_audit(&self, tokens: Vec<Self::AuditShare>) -> bool;
}

#[derive(Debug, Clone, Copy)]
struct InsecureProtocol {
    parties: usize,
}

impl InsecureProtocol {
    fn new(parties: usize) -> InsecureProtocol {
        InsecureProtocol { parties }
    }
}

impl Protocol for InsecureProtocol {
    type Message = u8;
    type ChannelKey = String;
    type WriteToken = (u8, Option<String>); // message + maybe a key
    type AuditShare = bool;

    fn num_parties(&self) -> usize {
        self.parties
    }

    fn broadcast(&self, message: u8, key: String) -> Vec<(u8, Option<String>)> {
        let mut data = vec![(u8::default(), None); self.parties - 1];
        data.push((message, Some(key)));
        data
    }

    fn null_broadcast(&self) -> Vec<(u8, Option<String>)> {
        vec![(u8::default(), None); self.parties]
    }

    fn gen_audit(&self, key: &String, token: (u8, Option<String>)) -> Vec<bool> {
        let (data, proof) = token;
        vec![data == 0 || proof.as_ref() == Some(key); self.parties]
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

    fn protocols() -> impl Strategy<Value = InsecureProtocol> {
        (2usize..100usize).prop_map(InsecureProtocol::new)
    }

    fn keys() -> impl Strategy<Value = String> {
        Just("password".to_string())
    }

    fn bad_keys() -> impl Strategy<Value = String> {
        "\\PC+".prop_filter("Must not be the actual key!", |s| s != "password")
    }

    fn messages() -> impl Strategy<Value = u8> {
        any::<u8>()
    }

    fn get_server_shares(
        protocol: InsecureProtocol,
        tokens: Vec<(u8, Option<String>)>,
        key: String,
    ) -> Vec<Vec<bool>> {
        let mut server_shares: Vec<Vec<bool>> = vec![Vec::new(); protocol.num_parties()];
        for token in tokens {
            for (idx, share) in protocol.gen_audit(&key, token).into_iter().enumerate() {
                server_shares[idx].push(share);
            }
        }
        server_shares
    }

    proptest! {
        #[test]
        fn test_null_broadcast_passes_audit(protocol in protocols(), key in keys()) {
            let tokens = protocol.null_broadcast();
            prop_assert_eq!(tokens.len(), protocol.num_parties());

            for shares in get_server_shares(protocol, tokens, key) {
                prop_assert!(protocol.check_audit(shares));
            }
        }

        #[test]
        fn test_broadcast_passes_audit(protocol in protocols(), message in messages(), key in keys()) {
            let tokens = protocol.broadcast(message, key.clone());
            prop_assert_eq!(tokens.len(), protocol.num_parties());

            for shares in get_server_shares(protocol, tokens, key) {
                prop_assert!(protocol.check_audit(shares));
            }
        }

        #[test]
        fn test_broadcast_bad_key_fails_audit(
            protocol in protocols(),
            message in messages().prop_filter("Broadcasting null message okay!", |m| *m != u8::default()),
            good_key in keys(),
            bad_key in bad_keys()
        ) {
            let tokens = protocol.broadcast(message, bad_key);
            prop_assert_eq!(tokens.len(), protocol.num_parties());

            for shares in get_server_shares(protocol, tokens, good_key) {
                prop_assert!(!protocol.check_audit(shares));
            }
        }
    }
}
