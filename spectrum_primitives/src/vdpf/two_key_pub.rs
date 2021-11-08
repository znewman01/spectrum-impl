use crate::algebra::{Monoid, SpecialExponentMonoid};
use crate::bytes::Bytes;
use crate::constructions::jubjub::{CurvePoint, Scalar};
use crate::constructions::AesSeed;
use crate::dpf::Dpf;
use crate::dpf::TwoKeyDpf;
use crate::prg::Prg;
use crate::util::Sampleable;
use crate::vdpf::Vdpf;

use std::fmt::Debug;
use std::iter::repeat_with;
use std::ops::{BitXor, BitXorAssign};
use std::sync::Arc;
use std::{convert::TryInto, ops::Add};

use serde::{Deserialize, Serialize};

#[cfg(any(test, feature = "testing"))]
use proptest::prelude::*;
#[cfg(any(test, feature = "testing"))]
use proptest_derive::Arbitrary;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct KeyPair {
    public: CurvePoint,
    private: Scalar,
}

impl Sampleable for KeyPair {
    type Seed = AesSeed;

    fn sample() -> Self {
        Scalar::sample().into()
    }

    fn sample_many_from_seed(seed: &Self::Seed, n: usize) -> Vec<Self>
    where
        Self: Sized,
    {
        Scalar::sample_many_from_seed(seed, n)
            .into_iter()
            .map(Into::into)
            .collect()
    }
}

#[cfg(any(test, feature = "testing"))]
impl Arbitrary for KeyPair {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;
    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        any_with::<Scalar>(()).prop_map(KeyPair::from).boxed()
    }
}

impl From<Scalar> for KeyPair {
    fn from(private: Scalar) -> Self {
        let public: CurvePoint = private.clone().into();
        Self { public, private }
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Arbitrary))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProofShare {
    seed: CurvePoint,
    bit: CurvePoint,
}

impl ProofShare {
    pub fn new(seed: CurvePoint, bit: CurvePoint) -> Self {
        ProofShare { seed, bit }
    }
}

impl ProofShare {
    pub fn bit(&self) -> CurvePoint {
        self.bit.clone()
    }

    pub fn seed(&self) -> CurvePoint {
        self.seed.clone()
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Arbitrary))]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Token {
    seed: CurvePoint,
    bit: CurvePoint,
    data: Bytes,
}

impl Token {
    pub fn new(seed: CurvePoint, bit: CurvePoint, data: Bytes) -> Self {
        Token { seed, bit, data }
    }

    pub fn data(&self) -> Bytes {
        self.data.clone()
    }
}

impl Token {
    pub fn seed(&self) -> CurvePoint {
        self.seed.clone()
    }

    pub fn bit(&self) -> CurvePoint {
        self.bit.clone()
    }
}

impl From<Token> for ProofShare {
    fn from(token: Token) -> Self {
        ProofShare {
            seed: token.seed,
            bit: token.bit,
        }
    }
}

#[cfg_attr(any(test, feature = "testing"), derive(Arbitrary))]
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Construction<D> {
    dpf: D,
}

impl<D> Construction<D> {
    pub fn new(dpf: D) -> Self {
        Self { dpf }
    }
}

impl<D> Dpf for Construction<D>
where
    D: Dpf,
{
    type Key = D::Key;

    type Message = D::Message;

    fn points(&self) -> usize {
        self.dpf.points()
    }

    fn keys(&self) -> usize {
        self.dpf.keys()
    }

    fn null_message(&self) -> Self::Message {
        self.dpf.null_message()
    }

    fn msg_size(&self) -> usize {
        self.dpf.msg_size()
    }

    fn gen(&self, msg: Self::Message, idx: usize) -> Vec<Self::Key> {
        self.dpf.gen(msg, idx)
    }

    fn gen_empty(&self) -> Vec<Self::Key> {
        self.dpf.gen_empty()
    }

    fn eval(&self, key: Self::Key) -> Vec<Self::Message> {
        self.dpf.eval(key)
    }

    fn combine(&self, parts: Vec<Vec<Self::Message>>) -> Vec<Self::Message> {
        self.dpf.combine(parts)
    }
}

impl<P> Vdpf for Construction<TwoKeyDpf<P>>
where
    P: Prg + Clone,
    P::Seed: Clone + Debug + Eq + TryInto<Scalar>,
    <P::Seed as TryInto<Scalar>>::Error: Debug,
    P::Output: Debug
        + Eq
        + Clone
        + AsRef<[u8]>
        + BitXor<P::Output, Output = P::Output>
        + BitXor<Arc<P::Output>, Output = P::Output>
        + BitXorAssign<P::Output>,
{
    type AuthKey = KeyPair;
    type ProofShare = ProofShare;
    type Token = Token;

    fn new_access_key(&self) -> Self::AuthKey {
        KeyPair::from(Scalar::sample())
    }

    fn new_access_keys(&self) -> Vec<Self::AuthKey> {
        repeat_with(Scalar::sample)
            .map(KeyPair::from)
            .take(self.points())
            .collect()
    }

    fn gen_proofs(
        &self,
        auth_key: &Self::AuthKey,
        idx: usize,
        dpf_keys: &[<Self as Dpf>::Key],
    ) -> Vec<Self::ProofShare> {
        assert_eq!(dpf_keys.len(), 2, "not implemented");
        // Server i computes: <auth_keys> . <bits[i]> + bit_proofs[i]
        // Then servers 1 and 2 check results equal.
        //
        // We know bits[i] are the same, except at bits[i][idx].
        // So we really just need:
        // auth_key ^ bits[1][idx] + proofs[1] == auth_key ^ bits[0][idx] + proofs[0]
        let mut bit_a = CurvePoint::sample();
        let mut bit_b = bit_a.clone();
        if dpf_keys[0].bits[idx] {
            bit_b = bit_b + auth_key.private.into(); // into() -> exponentiation!
        } else {
            bit_a = bit_a + auth_key.private.into(); // into() -> exponentiation!
        }

        // Similar here for seeds instead of bits, but we don't have seeds in {0, 1}. Want:
        // proofs[1] == proofs[0] + auth_key ^ (seeds[0][idx] - seeds[1][idx])
        let mut seed_a = CurvePoint::sample();
        let mut seed_b = seed_a.clone();
        seed_a =
            seed_a + (dpf_keys[1].seeds[idx].clone().try_into().unwrap() * auth_key.private).into(); // into() -> exp
        seed_b =
            seed_b + (dpf_keys[0].seeds[idx].clone().try_into().unwrap() * auth_key.private).into(); // into() -> exp

        vec![
            ProofShare::new(seed_a, bit_a),
            ProofShare::new(seed_b, bit_b),
        ]
    }

    fn gen_proofs_noop(&self) -> Vec<Self::ProofShare> {
        use std::iter::repeat;
        // Same random values
        repeat(ProofShare {
            seed: CurvePoint::sample(),
            bit: CurvePoint::sample(),
        })
        .take(2)
        .collect()
    }

    fn gen_audit(
        &self,
        auth_keys: &[Self::AuthKey],
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
            .map(
                |(bit, key)| {
                    if *bit {
                        key.public
                    } else {
                        CurvePoint::zero()
                    }
                },
            )
            .fold(CurvePoint::zero(), Add::add)
            + proof_share.bit;

        // Inner product + proof share
        let seed_check = dpf_key
            .seeds
            .iter()
            .map(|seed| seed.clone().try_into().unwrap())
            .zip(auth_keys)
            .map(|(seed, key)| key.public.pow(seed))
            .fold(CurvePoint::zero(), Add::add)
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
        // tokens[0] == tokens[1]
        tokens[0].seed == tokens[1].seed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructions::AesPrg;

    check_vdpf!(Construction<TwoKeyDpf<AesPrg>>);
}
