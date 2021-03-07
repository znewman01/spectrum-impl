use crate::dpf::{insecure::Construction, DPF};
use crate::vdpf::VDPF;
use std::iter::repeat;

impl<M> VDPF for Construction<M>
where
    M: Default + Clone + Eq,
{
    // true is a valid auth key; false is not
    type AuthKey = bool;
    // true authorizes a non-zero write; false does not
    type ProofShare = bool;
    // true is okay; false is not
    type Token = bool;

    fn new_access_key(&self) -> Self::AuthKey {
        true
    }

    fn new_access_keys(&self) -> Vec<Self::AuthKey> {
        repeat(true).take(self.points()).collect()
    }

    fn gen_proofs(
        &self,
        auth_key: &Self::AuthKey,
        _point_idx: usize,
        dpf_keys: &[<Self as DPF>::Key],
    ) -> Vec<Self::ProofShare> {
        dpf_keys.iter().map(|_| *auth_key).collect()
    }

    fn gen_proofs_noop(&self) -> Vec<Self::ProofShare> {
        repeat(false).take(self.keys()).collect()
    }

    fn gen_audit(
        &self,
        _auth_keys: &[Self::AuthKey],
        dpf_key: &<Self as DPF>::Key,
        proof_share: Self::ProofShare,
    ) -> Self::Token {
        proof_share || dpf_key.is_none()
    }

    fn check_audit(&self, tokens: Vec<Self::Token>) -> bool {
        tokens.iter().all(|x| *x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dpf::insecure::Message;
    check_vdpf!(Construction<Message>);
}
