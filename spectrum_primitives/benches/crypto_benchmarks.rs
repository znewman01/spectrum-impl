use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use rand::thread_rng;
use spectrum_primitives::{
    bytes::Bytes,
    dpf::{BasicDPF, DPF},
    prg::{aes::AESPRG, PRG},
};
use std::thread::sleep;
use std::time::Duration;

fn criterion_benchmark(c: &mut Criterion) {
    static KB: usize = 1000;
    static MB: usize = 1000000;
    static SIZES: [usize; 6] = [KB, 10 * KB, 100 * KB, 250 * KB, 500 * KB, 1 * MB];

    // Bytes per second of AES on Zack's laptop via `openssl speed`.
    // TODO: run inline while we're collecting benchmarks
    static AES_RATE: u64 = 3500000000;
    static NS_PER_S: u64 = 1000000000; // microseconds per second

    let mut group = c.benchmark_group("AESPRG");
    for size in SIZES.iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::new("PRG", size), size, |b, &size| {
            let prg = AESPRG::new(16, size);
            let seed = prg.new_seed();
            b.iter_with_large_drop(|| prg.eval(&seed))
        });
        if *size >= 100 * KB {
            // We know, roughly, the max rate of AES on our system. We want to
            // have that on the plots to compare against, but there's no easy
            // way to just add a line. Instead, we fake it.
            group.bench_with_input(BenchmarkId::new("Max AES Rate", size), size, |b, &size| {
                let delay = Duration::from_nanos(NS_PER_S * (size as u64) / AES_RATE / 2);
                b.iter_with_large_drop(|| sleep(delay))
            });
        }
    }
    group.finish();

    let mut group = c.benchmark_group("DPF (AES) Evaluation");
    for size in SIZES.iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let dpf = BasicDPF::new(AESPRG::new(16, size), 1);
            let keys = dpf.gen_empty();
            let key = &keys[0];
            b.iter_with_large_drop(|| dpf.eval(key))
        });
    }
    group.finish();

    let mut group = c.benchmark_group("XOR");
    for size in SIZES.iter() {
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
