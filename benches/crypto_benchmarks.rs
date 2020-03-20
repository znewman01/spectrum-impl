#[macro_use]
extern crate criterion;

use criterion::Criterion;
use rug::Integer;
use spectrum_impl::crypto::{
    byte_utils::Bytes,
    dpf::{PRGBasedDPF, DPF},
    field::Field,
    prg::{AESPRG, PRG},
    vdpf::VDPF,
};
use std::rc::Rc;

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("PRG eval benchmark", |b| {
        let eval_size: usize = 1 << 20; // approx 1MB
        let prg = AESPRG::new();
        let seed = prg.new_seed();
        b.iter(|| {
            // benchmark the PRG evaluation time
            prg.eval(&seed, eval_size);
        })
    });

    c.bench_function("PRGBasedDPF eval benchmark", |b| {
        let eval_size: usize = 1 << 20; // approx 1MB
        let dpf = PRGBasedDPF::new(AESPRG::new(), 16, 2, 1);
        let keys = dpf.gen(&Bytes::empty(eval_size), 0);
        b.iter(|| {
            // benchmark the DPF (PRG-based) evaluation time
            dpf.eval(&keys[0]);
        })
    });

    c.bench_function("gen_audit", |b| {
        let eval_size: usize = 1 << 20; // approx 1MB
        let point_idx = 0;
        let num_points = 1;
        let vdpf = PRGBasedDPF::new(AESPRG::new(), 16, 2, num_points);

        // setup a field for the VDPF auth
        let mut p = Integer::from(800_000_000);
        p.next_prime_mut();
        let field = Rc::<Field>::new(Field::new(p));

        // generate dpf keys
        let dpf_keys = vdpf.gen(&Bytes::empty(eval_size), point_idx);

        // generate null authentication keys for the vdpf
        let auth_keys = vec![field.zero(); num_points];

        // generate the proof shares for the VDPF
        let proof_shares = vdpf.gen_proofs(&auth_keys[point_idx], point_idx, &dpf_keys);

        b.iter(|| {
            // benchmark the gen_audit function of the VDPF
            vdpf.gen_audit(&auth_keys, &dpf_keys[0], &proof_shares[0])
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
