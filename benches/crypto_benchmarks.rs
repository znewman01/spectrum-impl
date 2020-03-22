#[macro_use]
extern crate criterion;

use criterion::Criterion;
use rug::Integer;
use spectrum_impl::{
    bytes::Bytes,
    crypto::{
        dpf::{DPF, PRGDPF},
        field::Field,
        prg::{AESPRG, PRG},
        vdpf::{FieldVDPF, VDPF},
    },
};
use std::sync::Arc;

const EVAL_SIZE: usize = 1 << 20; // approx 1MB

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("PRG eval benchmark", |b| {
        let prg = AESPRG::new();
        let seed = prg.new_seed();
        b.iter(|| prg.eval(&seed, EVAL_SIZE))
    });

    let num_keys = 2;
    let num_points = 1;
    let point_idx = 0;
    c.bench_function("PRGDPF eval benchmark", |b| {
        let dpf = PRGDPF::new(AESPRG::new(), num_keys, num_points);
        let data = Bytes::empty(EVAL_SIZE);
        let keys = dpf.gen(&data, point_idx);
        let key = &keys[0];
        b.iter(|| dpf.eval(key))
    });

    let point_idx = 0;
    let num_keys = 2;
    let num_points = 1;
    let prime: Integer = Integer::from(800_000_000).next_prime_ref().into();
    c.bench_function("gen_audit", |b| {
        let field = Arc::new(Field::new(prime.clone()));
        let dpf = PRGDPF::new(AESPRG::new(), num_keys, num_points);
        let vdpf = FieldVDPF::new(dpf, field.clone());

        let data = Bytes::empty(EVAL_SIZE);
        let dpf_keys = vdpf.gen(&data, point_idx);
        let auth_keys = vec![field.zero(); 2];
        let proof_shares = vdpf.gen_proofs(&auth_keys[point_idx], point_idx, &dpf_keys);

        let dpf_key = &dpf_keys[0];
        let proof_share = &proof_shares[0];
        b.iter(|| vdpf.gen_audit(&auth_keys, dpf_key, proof_share))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
