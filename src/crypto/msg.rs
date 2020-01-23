//! Spectrum implementation.
use rand::{OsRng, Rng};
use std::fmt::Debug;
use std::ops;

/// message contains a vector of bytes representing data in spectrum
/// and is used for easily performing binary operations over bytes
#[derive(Clone, PartialEq, Debug)]
pub struct Message {
    pub data: Vec<u8>,
}

impl Message {
    /// creates a new message type
    pub fn new(data: Vec<u8>) -> Message {
        Message { data: data }
    }
}

/// xors the data of the two messages
impl ops::BitXor<Message> for Message {
    type Output = Message;

    fn bitxor(self, other: Message) -> Message {
        assert_eq!(self.data.len(), other.data.len());

        let xor_data: Vec<u8> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(&a, &b)| a ^ b)
            .collect();

        Message { data: xor_data }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msg_xor_op() {
        let mut data1 = vec![0; 2048];
        let mut data2 = vec![0; 2048];
        let mut rng = OsRng::new().ok().unwrap();
        rng.fill_bytes(&mut data1);
        rng.fill_bytes(&mut data2);

        let xor_data_expected: Vec<u8> = data1
            .iter()
            .zip(data2.iter())
            .map(|(&a, &b)| a ^ b)
            .collect();

        let msg1 = Message::new(data1.clone());
        let msg2 = Message::new(data2.clone());

        let msg_xor = msg1 ^ msg2;

        assert_eq!(msg_xor.data, xor_data_expected);
    }
}
