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
    fn gen_audit(&self, key: Self::ChannelKey, token: Self::WriteToken) -> Vec<Self::AuditShare>;
    fn check_audit(&self, tokens: Vec<Self::AuditShare>) -> bool;
}

#[derive(Debug, Clone, Copy)]
struct InsecureProtocol {
    parties: usize,
    msg_len: usize,
}

impl InsecureProtocol {
    fn new(parties: usize, msg_len: usize) -> InsecureProtocol {
        InsecureProtocol { parties, msg_len }
    }
}

impl Protocol for InsecureProtocol {
    type Message = Vec<u8>;
    type ChannelKey = ();
    type WriteToken = Vec<u8>;
    type AuditShare = ();

    fn num_parties(&self) -> usize {
        self.parties
    }

    fn broadcast(&self, message: Vec<u8>, _key: ()) -> Vec<Vec<u8>> {
        let null_message = vec![0u8; self.msg_len];
        let mut data = vec![null_message; self.parties - 1];
        data.push(message);
        data
    }

    fn null_broadcast(&self) -> Vec<Vec<u8>> {
        let null_message = vec![0u8; self.msg_len];
        vec![null_message; self.parties]
    }

    fn gen_audit(&self, _key: (), _token: Vec<u8>) -> Vec<()> {
        vec![(); self.parties]
    }

    fn check_audit(&self, tokens: Vec<()>) -> bool {
        assert_eq!(tokens.len(), self.parties);
        true
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unit_arg)] // we *want* unit values for channel keys
    use super::*;
    use proptest::prelude::*;

    fn protocols() -> impl Strategy<Value = InsecureProtocol> {
        (2usize..100usize, 0usize..1000usize).prop_map(|(p, l)| InsecureProtocol::new(p, l))
    }

    fn keys() -> impl Strategy<Value = ()> {
        Just(())
    }

    fn protocols_and_messages() -> impl Strategy<Value = (InsecureProtocol, Vec<u8>)> {
        protocols().prop_flat_map(|protocol| {
            (
                Just(protocol),
                prop::collection::vec(any::<u8>(), protocol.msg_len),
            )
        })
    }

    proptest! {
        #[test]
        fn test_null_broadcast_passes_audit(protocol in protocols(), key in keys()) {
            let write_tokens = protocol.null_broadcast();
            prop_assert_eq!(write_tokens.len(), protocol.num_parties());

            for token in write_tokens {
                let audit_shares = protocol.gen_audit(key, token);
                prop_assert_eq!(audit_shares.len(), protocol.num_parties());
                prop_assert!(protocol.check_audit(audit_shares));
            }
        }

        #[test]
        fn test_broadcast_passes_audit((protocol, message) in protocols_and_messages(), key in keys()) {
            let write_tokens = protocol.broadcast(message, key);
            prop_assert_eq!(write_tokens.len(), protocol.num_parties());

            for token in write_tokens {
                let audit_shares = protocol.gen_audit(key, token);
                prop_assert_eq!(audit_shares.len(), protocol.num_parties());
                prop_assert!(protocol.check_audit(audit_shares));
            }
        }
    }

    // test: broadcast with bad key does not pass audit (not true for insecure protocol)
}
