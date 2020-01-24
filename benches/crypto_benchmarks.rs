#[macro_use]
extern crate criterion;

use criterion::Criterion;
use spectrum_impl::crypto::prg::PRG;

fn criterion_benchmark(c: &mut Criterion) {
    let seed_size: usize = 16;
    let eval_size: usize = 1 << 20; // approx 1M bytes
    let prg = PRG::new(seed_size, eval_size);
    let seed = prg.new_seed();

    c.bench_function("PRG eval benchmark", |b| b.iter(|| {
        // benchmark the PRG evaluation time
        prg.eval(&seed);
    }));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);