//! Spectrum implementation.
#![allow(dead_code)]

extern crate rand;
use crate::crypto::byte_utils::{xor_all_bytes, xor_bytes_list};
use crate::crypto::dpf::{DPFKey, PRGBasedDPF, DPF};
use bytes::Bytes;
use rand::prelude::*;

#[derive(Clone, PartialEq, Debug)]
struct CryptoParams {
    num_channels: usize,
    num_servers: usize,
    dpf: PRGBasedDPF,
}

// TODO: make sure it matches with protobufs
#[derive(Clone, PartialEq, Debug)]
struct ServerAuditToken {
    token: Bytes,
}

#[derive(Clone, PartialEq, Debug)]
struct ClientProofShare {
    share: Bytes,
}

impl CryptoParams {
    pub fn new(num_channels: usize, num_servers: usize, dpf: PRGBasedDPF) -> CryptoParams {
        CryptoParams {
            num_channels,
            num_servers,
            dpf,
        }
    }

    pub fn gen_audit_token(
        &self,
        channel_keys: Vec<Bytes>,
        dpf_key: DPFKey,
        client_proof: ClientProofShare,
    ) -> ServerAuditToken {
        assert_eq!(channel_keys.len(), dpf_key.bits.len());
        assert_eq!(channel_keys.len(), dpf_key.seeds.len());

        let mut token = self.dpf.compressed_eval(&dpf_key, &channel_keys);
        token = xor_bytes(&token, &client_proof.share);

        ServerAuditToken { token }
    }

    pub fn check_audit(&self, audit_shares: Vec<ServerAuditToken>) -> bool {
        let mut sum = audit_shares[0].token.clone();
        for share in audit_shares.iter().skip(1) {
            sum = xor_bytes(&sum, &share.token);
        }

        // check if sum is all-zero
        sum == Bytes::from(vec![0; sum.len()])
    }

    pub fn gen_proof_shares(
        &self,
        channel_key: Bytes,
        dpf_keys: Vec<DPFKey>,
    ) -> Vec<ClientProofShare> {
        let channel_keys = vec![channel_key; self.num_channels]; // duplicate channel_key across all channels
        let eval_bytes: Vec<Bytes> = dpf_keys
            .iter()
            .map(|key| self.dpf.compressed_eval(key, &channel_keys))
            .collect();
        let proof = xor_bytes_list(eval_bytes);

        let mut shares: Vec<ClientProofShare> = Vec::new();

        // sum of the random field elements
        let mut rand_sum = Bytes::from(vec![0; proof.len()]);
        for _ in 0..self.num_servers - 1 {
            let mut rand = vec![0; rand_sum.len()];
            thread_rng().fill_bytes(&mut rand);

            shares.push(ClientProofShare {
                share: Bytes::from(rand.clone()),
            });

            rand_sum = xor_bytes(&rand_sum, &Bytes::from(rand));
        }

        // last share set to proof + SUM rand
        shares.push(ClientProofShare {
            share: xor_bytes(&proof, &rand_sum),
        });

        shares
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_chan_key(security_bytes: usize) -> Bytes {
        let mut rand = vec![0; security_bytes];
        thread_rng().fill_bytes(&mut rand);
        Bytes::from(rand)
    }

    #[test]
    fn test_proof_check() {
        let num_chan = 10;
        let num_servers = 2;
        let chan_idx = 1;
        let sec_bytes = 16;

        let channel_keys = vec![random_chan_key(sec_bytes); num_chan];
        let dpf = PRGBasedDPF::new(sec_bytes, num_servers, 10);
        let dpf_keys = dpf.gen(Bytes::from(vec![0; sec_bytes * 2]), chan_idx);
        let params = CryptoParams::new(num_chan, num_servers, dpf);

        let client_proof_shares =
            params.gen_proof_shares(channel_keys[chan_idx].clone(), dpf_keys.clone());

        let audit_res = dpf_keys
            .iter()
            .zip(client_proof_shares.iter())
            .map(|(key, proof_share)| {
                params.gen_audit_token(channel_keys.clone(), key.clone(), proof_share.clone())
            })
            .collect();

        assert_eq!(params.check_audit(audit_res), true);
    }
}
