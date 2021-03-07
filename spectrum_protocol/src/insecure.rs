use crate::Protocol;
use spectrum_primitives::Bytes;

use serde::{Deserialize, Serialize};

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;

#[cfg(feature = "proto")]
use {crate::proto, std::convert::TryInto};

#[derive(Debug, Clone)]
pub struct WriteToken {
    data: Bytes,
    idx: usize,
    key: String,
}

impl WriteToken {
    fn new(data: Bytes, idx: usize, key: String) -> Self {
        WriteToken { data, idx, key }
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
    type ChannelKey = String; // password
    type WriteToken = Option<WriteToken>; // message, index, maybe a key
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
        let mut data = vec![None; self.parties - 1];
        data.push(Some(WriteToken::new(message, idx, key)));
        data
    }

    fn cover(&self) -> Vec<Self::WriteToken> {
        vec![None; self.parties]
    }

    fn gen_audit(
        &self,
        keys: &[Self::ChannelKey],
        token: Option<WriteToken>,
    ) -> Vec<Self::AuditShare> {
        let audit_share = match token {
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

    fn to_accumulator(&self, token: Option<WriteToken>) -> Vec<Bytes> {
        let mut accumulator = vec![Bytes::empty(self.message_len); self.num_channels() - 1];
        if let Some(inner) = token {
            accumulator.insert(inner.idx, inner.data);
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
