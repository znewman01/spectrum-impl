#![allow(clippy::identity_op)]
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use rand::thread_rng;
use spectrum_primitives::pir;
use spectrum_primitives::{Bytes, Dpf, MultiKeyVdpf, TwoKeyVdpf, Vdpf};
use std::fmt::{self, Display};
use std::iter::repeat_with;

#[derive(Copy, Clone, Debug)]
struct PirParams {
    channels: usize,
    size: usize,
}

impl Display for PirParams {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    static KB: usize = 1000;
    static MB: usize = 1000000;
    static GB: usize = 1000000000;
    static SIZES: [usize; 6] = [KB, 10 * KB, 100 * KB, 250 * KB, 500 * KB, 1 * MB];
    static CHANNELS: [usize; 6] = [1, 10, 100, 1000, 10000, 100000];

    let mut group = c.benchmark_group("DPF (AES) Evaluation");
    for size in SIZES.iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let dpf = TwoKeyVdpf::with_channels_msg_size(1, size);
            let keys = dpf.gen_empty();
            let key = &keys[0];
            b.iter_batched(|| key.clone(), |key| dpf.eval(key), BatchSize::LargeInput)
        });
    }
    group.finish();

    let mut group = c.benchmark_group("DPF (SH) Evaluation");
    for size in SIZES.iter() {
        let size = size / 10;
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let dpf = MultiKeyVdpf::with_channels_parties_msg_size(1, 3, size);
            let keys = dpf.gen_empty();
            let key = &keys[0];
            b.iter_batched(|| key.clone(), |key| dpf.eval(key), BatchSize::LargeInput)
        });
    }
    group.finish();

    let mut group = c.benchmark_group("XOR");
    for size in SIZES.iter().take(3) {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::new("Bytes", size), size, |b, &size| {
            b.iter_batched(
                || {
                    (
                        Bytes::random(size, &mut thread_rng()),
                        Bytes::random(size, &mut thread_rng()),
                    )
                },
                |(left, right)| left ^ right,
                BatchSize::LargeInput,
            )
        });
        group.bench_with_input(BenchmarkId::new("Vec<u8>()", size), size, |b, &size| {
            b.iter_batched_ref(
                || (vec![0; size], vec![0; size]),
                |(left, right)| {
                    left.iter_mut()
                        .zip(right.iter())
                        .for_each(|(l, r)| *l ^= *r)
                },
                BatchSize::LargeInput,
            )
        });
        // TODO: try with chunking
    }
    group.finish();

    let mut group = c.benchmark_group("Vdpf.gen_audit() (AES)");
    for size in SIZES.iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let vdpf = TwoKeyVdpf::with_channels_msg_size(1, size);
            let auth_keys = vdpf.new_access_keys();
            let dpf_keys = vdpf.gen_empty();
            let proof_shares = vdpf.gen_proofs_noop();
            let dpf_key = &dpf_keys[0];
            let proof_share = &proof_shares[0];
            b.iter_batched(
                || proof_share.clone(),
                |proof| vdpf.gen_audit(&auth_keys, dpf_key, proof),
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();

    let mut group = c.benchmark_group("Vdpf.gen_audit() (SH)");
    for size in SIZES.iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let vdpf = MultiKeyVdpf::with_channels_parties_msg_size(1, 3, size);
            let auth_keys = vdpf.new_access_keys();
            let dpf_keys = vdpf.gen_empty();
            let proof_shares = vdpf.gen_proofs_noop();
            let dpf_key = &dpf_keys[0];
            let proof_share = &proof_shares[0];
            b.iter_batched(
                || proof_share.clone(),
                |proof| vdpf.gen_audit(&auth_keys, dpf_key, proof),
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();

    let mut group = c.benchmark_group("PIR");
    for &size in SIZES.iter().take(3) {
        for &channels in CHANNELS.iter() {
            if channels * size > 1 * GB {
                continue;
            }
            let params = PirParams { size, channels };
            // group.throughput(Throughput::Bytes(*(size * channels) as u64));
            group.bench_with_input(
                BenchmarkId::from_parameter(params),
                &params,
                |b, &params| {
                    let channels = params.channels;
                    let size = params.size;
                    const IDX: usize = 0;
                    let data: Vec<Vec<u8>> = repeat_with(|| Bytes::random(size, &mut thread_rng()))
                        .take(channels)
                        .map(Vec::<u8>::from)
                        .collect();
                    let db = pir::LinearDatabase::from_vec(data);
                    b.iter_batched(
                        || {
                            <pir::LinearDatabase as pir::Database<2>>::queries(IDX, channels)
                                .unwrap()[0]
                                .clone()
                        },
                        |query| <pir::LinearDatabase as pir::Database<2>>::answer(&db, query),
                        BatchSize::LargeInput,
                    )
                },
            );
        }
    }
    group.finish();
    // TODO: more benchmarks
    // - GroupPRG
    // - DPF with Group PRG
    // - Group/Field Ops
    // - VDPF Features
}

// TODO: integrate profiling?
// https://www.jibbow.com/posts/criterion-flamegraphs/

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
