use crate::proto::{AuditShare, WriteToken};
use crate::protocols::Protocol;

pub trait ProtocolWrapper {
    // fn broadcast(&self, message: Self::Message, key: Self::ChannelKey) -> Vec<elf::WriteToken>;
    fn null_broadcast(&self) -> Vec<WriteToken>;

    // fn gen_audit(
    //     &self,
    //     keys: &[Self::ChannelKey],
    //     token: &Self::WriteToken,
    // ) -> Vec<Self::AuditShare>;
    fn check_audit(&self, tokens: Vec<AuditShare>) -> bool;

    // fn new_accumulator(&self) -> Self::Accumulator {
    //     Self::Accumulator::new(self.num_channels())
    // }

    // fn to_accumulator(&self, token: Self::WriteToken) -> Self::Accumulator;
}

impl<P: Protocol> ProtocolWrapper for P {
    fn null_broadcast(&self) -> Vec<WriteToken> {
        self.null_broadcast().into_iter().map(Into::into).collect()
    }

    fn check_audit(&self, tokens: Vec<AuditShare>) -> bool {
        let tokens = tokens.into_iter().map(Into::into).collect();
        self.check_audit(tokens)
    }
}
