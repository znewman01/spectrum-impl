use std::iter::repeat_with;
use std::ops::Add;
use std::{fmt::Debug, iter::Sum};

use crate::algebra::{Field, Group, SpecialExponentMonoid};
use crate::bytes::Bytes;
use crate::dpf::Dpf;
use crate::dpf::MultiKeyDpf;
use crate::prg::GroupPrg;
use crate::sharing::Shareable;
use crate::util::Sampleable;

use super::*;

#[cfg(any(test, feature = "testing"))]
use proptest_derive::Arbitrary;

#[cfg_attr(any(test, feature = "testing"), derive(Arbitrary))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofShare<S> {
    bit: S,
    seed: S,
}

impl<S> ProofShare<S> {
    pub fn new(bit: S, seed: S) -> Self {
        ProofShare { bit, seed }
    }
}

impl<S> ProofShare<S>
where
    S: Clone,
{
    pub fn bit(&self) -> S {
        self.bit.clone()
    }
    pub fn seed(&self) -> S {
        self.seed.clone()
    }
}

impl<F> Shareable for ProofShare<F>
where
    F: Field + Shareable<Share = F>,
{
    type Share = ProofShare<F>;

    fn share(self, n: usize) -> Vec<Self::Share> {
        Iterator::zip(
            self.seed.share(n).into_iter(),
            self.bit.share(n).into_iter(),
        )
        .map(|(seed, bit)| ProofShare { bit, seed })
        .collect()
    }

    fn recover(shares: Vec<Self::Share>) -> Self {
        let (bits, seeds): (Vec<_>, Vec<_>) = shares.into_iter().map(|s| (s.bit, s.seed)).unzip();
        let bit = F::recover(bits);
        let seed = F::recover(seeds);
        ProofShare { bit, seed }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "testing"), derive(Arbitrary))]
pub struct Token<S> {
    seed: S,
    bit: S,
    data: Bytes,
}

impl<S> Token<S> {
    pub fn new(seed: S, bit: S, data: Bytes) -> Self {
        Token { seed, bit, data }
    }

    pub fn data(&self) -> Bytes {
        self.data.clone()
    }
}

impl<S> Token<S>
where
    S: Clone,
{
    pub fn seed(&self) -> S {
        self.seed.clone()
    }

    pub fn bit(&self) -> S {
        self.bit.clone()
    }
}

impl<S> From<Token<S>> for ProofShare<S> {
    fn from(token: Token<S>) -> Self {
        ProofShare {
            seed: token.seed,
            bit: token.bit,
        }
    }
}

impl<G, F> Vdpf for FieldVdpf<MultiKeyDpf<GroupPrg<G>>, F>
where
    G: Shareable
        + Clone
        + Group
        + Debug
        + Sampleable
        + SpecialExponentMonoid<Exponent = F>
        + Into<Vec<u8>>,
    F: Sampleable + Field + Sum + Clone + Debug + Shareable<Share = F>,
{
    type AuthKey = F;
    type ProofShare = ProofShare<F>;
    type Token = Token<F>;

    /// samples a new access key for an index
    fn new_access_key(&self) -> F {
        F::sample()
    }

    /// samples a new set of access keys for range of indices
    fn new_access_keys(&self) -> Vec<F> {
        repeat_with(F::sample).take(self.points()).collect()
    }

    fn gen_proofs(
        &self,
        auth_key: &F,
        idx: usize,
        dpf_keys: &[<Self as Dpf>::Key],
    ) -> Vec<Self::ProofShare> {
        // Servers take inner product of bit and seed vectors, then sum and check == 0.
        // We need to share a "correction term" that makes that check out.
        // Our bit vector is 0 everywhere except at idx, where it's 1, so the inner product is just auth_key.
        // Our seed vector is 0 everywhere except at idx, so the inner product is auth_key * seed.
        let seed = dpf_keys
            .iter()
            .map(|k| k.seeds[idx].clone())
            .fold(F::zero(), Add::add);
        ProofShare::new(-auth_key.clone(), -(seed * auth_key.clone())).share(self.keys())
    }

    fn gen_proofs_noop(&self) -> Vec<Self::ProofShare> {
        // Share of zero values. Since our DPF isn't writing anything, we don't have anything to correct.
        ProofShare::new(F::zero(), F::zero()).share(self.keys())
    }

    fn gen_audit(
        &self,
        auth_keys: &[F],
        dpf_key: &<Self as Dpf>::Key,
        proof_share: ProofShare<F>,
    ) -> Token<F> {
        assert_eq!(auth_keys.len(), dpf_key.bits.len());
        assert_eq!(auth_keys.len(), dpf_key.seeds.len());

        // Inner product + proof share
        let bits = dpf_key.bits.iter().cloned();
        let bit_check = proof_share.bit.clone()
            + Iterator::zip(bits, auth_keys)
                .map(|(bit, key)| bit * key.clone())
                .fold(F::zero(), Add::add);

        // Inner product + proof share
        let seeds = dpf_key.seeds.iter().cloned();
        let seed_check = proof_share.seed
            + Iterator::zip(seeds, auth_keys)
                .map(|(seed, key)| seed * key.clone())
                .fold(F::zero(), Add::add);

        // TODO: kill this clone
        let msg_hash: Vec<u8> = dpf_key.encoded_msg.clone().hash_all();

        Token {
            bit: bit_check,
            seed: seed_check,
            data: msg_hash.into(),
        }
    }

    fn check_audit(&self, tokens: Vec<Self::Token>) -> bool {
        use std::collections::HashSet;
        // make sure all hashes are equal
        let distinct_hashes: HashSet<_> = tokens.iter().map(|t| t.data.clone()).collect();
        if distinct_hashes.len() != 1 {
            eprintln!("bad hashes {:?}", distinct_hashes);
            return false;
        }

        // and bit/seed checks sum to zero
        let proof = ProofShare::recover(tokens.into_iter().map(ProofShare::from).collect());
        if proof.bit != F::zero() || proof.seed != F::zero() {
            eprintln!("bad pf bits {:?}", proof);
            return false;
        }

        true
    }
}
