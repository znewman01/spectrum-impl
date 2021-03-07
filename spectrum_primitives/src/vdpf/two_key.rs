use crate::algebra::Field;
use crate::bytes::Bytes;
use crate::dpf::Dpf;
use crate::dpf::TwoKeyDpf;
use crate::lss::Shareable;
use crate::prg::Prg;
use crate::util::Sampleable;
use crate::vdpf::Vdpf;

use std::fmt::Debug;
use std::iter::repeat_with;
use std::ops::{BitXor, BitXorAssign};
use std::sync::Arc;
use std::{convert::TryInto, ops::Add};

use super::field::FieldVdpf;

#[derive(Clone)]
pub struct ProofShare<S> {
    seed: S,
    bit: S,
}

impl<S> ProofShare<S> {
    fn new(seed: S, bit: S) -> Self {
        ProofShare { seed, bit }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Token<S> {
    seed: S,
    bit: S,
    data: Bytes,
}

impl<S> From<Token<S>> for ProofShare<S> {
    fn from(token: Token<S>) -> Self {
        ProofShare {
            seed: token.seed,
            bit: token.bit,
        }
    }
}

impl<F, P> Vdpf for FieldVdpf<TwoKeyDpf<P>, F>
where
    F: Field + Sampleable + Clone + Shareable<Share = F>,
    P: Prg + Clone,
    P::Seed: Clone + Debug + Eq + TryInto<F>,
    <P::Seed as TryInto<F>>::Error: Debug,
    P::Output: Debug
        + Eq
        + Clone
        + AsRef<[u8]>
        + BitXor<P::Output, Output = P::Output>
        + BitXor<Arc<P::Output>, Output = P::Output>
        + BitXorAssign<P::Output>,
{
    type AuthKey = F;
    type ProofShare = ProofShare<F>;
    type Token = Token<F>;

    fn new_access_key(&self) -> Self::AuthKey {
        F::sample()
    }

    fn new_access_keys(&self) -> Vec<Self::AuthKey> {
        repeat_with(F::sample).take(self.points()).collect()
    }

    fn gen_proofs(
        &self,
        auth_key: &F,
        idx: usize,
        dpf_keys: &[<Self as Dpf>::Key],
    ) -> Vec<Self::ProofShare> {
        assert_eq!(dpf_keys.len(), 2, "not implemented");
        // Server i computes: <auth_keys> . <bits[i]> + bit_proofs[i]
        // Then servers 1 and 2 check results equal.
        //
        // We know bits[i] are the same, except at bits[i][idx].
        // So we really just need:
        // bits[1][idx] * auth_key + proofs[1] == bits[0][idx] * auth_key + proofs[0] <=>
        // proofs[1] == proofs[0] + (bits[0][idx] - bits[1][idx]) * auth_key
        let bit_a = F::sample();
        // Because bits[0][idx] is boolean, we just need to know whether to add/subtract auth_key.
        let mut bit_b = bit_a.clone();
        if dpf_keys[0].bits[idx] {
            bit_b = bit_b + auth_key.clone();
        } else {
            bit_b = bit_b - auth_key.clone();
        }

        // Similar here for seeds instead of bits, but we don't have seeds in {0, 1}. Want:
        // proofs[1] == proofs[0] + (seeds[0][idx] seeds[1][idx]) * auth_key
        let mut seed_a = F::sample();
        let mut seed_b = seed_a.clone();
        seed_a = seed_a + dpf_keys[1].seeds[idx].clone().try_into().unwrap() * auth_key.clone();
        seed_b = seed_b + dpf_keys[0].seeds[idx].clone().try_into().unwrap() * auth_key.clone();

        vec![
            ProofShare::new(seed_a, bit_a),
            ProofShare::new(seed_b, bit_b),
        ]
    }

    fn gen_proofs_noop(&self) -> Vec<Self::ProofShare> {
        use std::iter::repeat;
        // Same random values
        repeat(ProofShare {
            seed: F::sample(),
            bit: F::sample(),
        })
        .take(2)
        .collect()
    }

    fn gen_audit(
        &self,
        auth_keys: &[F],
        dpf_key: &<Self as Dpf>::Key,
        proof_share: Self::ProofShare,
    ) -> Self::Token {
        assert_eq!(auth_keys.len(), dpf_key.bits.len());
        assert_eq!(auth_keys.len(), dpf_key.seeds.len());
        // Inner product + proof share
        let bit_check = dpf_key
            .bits
            .iter()
            .zip(auth_keys)
            .map(|(bit, key)| if *bit { key.clone() } else { F::zero() })
            .fold(F::zero(), Add::add)
            + proof_share.bit;

        // Inner product + proof share
        let seed_check = dpf_key
            .seeds
            .iter()
            .map(|seed| seed.clone().try_into().unwrap())
            .zip(auth_keys)
            .map(|(seed, key)| seed * key.clone())
            .fold(F::zero(), Add::add)
            + proof_share.seed;

        // Hash of message
        let mut hasher = blake3::Hasher::new();
        let input: &[u8] = dpf_key.encoded_msg.as_ref();
        if input.len() >= 125000 {
            hasher.update_with_join::<blake3::join::RayonJoin>(input);
        } else {
            hasher.update(input);
        }
        let data: [u8; 32] = hasher.finalize().into();

        Token {
            bit: bit_check,
            seed: seed_check,
            data: Bytes::from(data.to_vec()),
        }
    }

    fn check_audit(&self, tokens: Vec<Self::Token>) -> bool {
        assert_eq!(tokens.len(), 2, "not implemented");
        tokens[0] == tokens[1]
    }
}
