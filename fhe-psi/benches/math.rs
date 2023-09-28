use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fhe_psi::math::gadget::{base_from_len, gadget_inverse};
use fhe_psi::math::int_mod_cyclo::IntModCyclo;
use fhe_psi::math::int_mod_cyclo_crt::IntModCycloCRT;
use fhe_psi::math::int_mod_cyclo_crt_eval::IntModCycloCRTEval;
use fhe_psi::math::int_mod_cyclo_eval::IntModCycloEval;
use fhe_psi::math::matrix::Matrix;
use fhe_psi::math::ntt::{ntt_neg_backward, ntt_neg_forward};
use fhe_psi::math::number_theory::find_sqrt_primitive_root;
use fhe_psi::math::rand_sampled::RandUniformSampled;
use fhe_psi::math::utils::{floor_log, mod_inverse};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("math::ntt_neg_forward", |b| {
        const D: usize = 2048;
        const P: u64 = 268369921;
        const W: u64 = find_sqrt_primitive_root(D, P);
        type RCoeff = IntModCyclo<D, P>;

        let mut rng = ChaCha20Rng::from_entropy();
        let elem = RCoeff::rand_uniform(&mut rng);
        let mut points = elem.into_aligned();
        b.iter(|| {
            ntt_neg_forward::<D, P, W>(black_box(&mut points));
        });
    });

    c.bench_function("math::ntt_neg_backward", |b| {
        const D: usize = 2048;
        const P: u64 = 268369921;
        const W: u64 = find_sqrt_primitive_root(D, P);
        type RCoeff = IntModCyclo<D, P>;

        let mut rng = ChaCha20Rng::from_entropy();
        let elem = RCoeff::rand_uniform(&mut rng);
        let mut points = elem.into_aligned();
        b.iter(|| {
            ntt_neg_backward::<D, P, W>(black_box(&mut points));
        });
    });

    const Q_A: u64 = 268369921;
    const Q_B: u64 = 249561089;
    const Q: u64 = Q_A * Q_B;

    const D: usize = 2048;

    type Ring = IntModCyclo<D, Q_A>;

    type RingEval = IntModCycloEval<D, Q_A, { find_sqrt_primitive_root(D, Q_A) }>;

    type RingCRT =
        IntModCycloCRT<D, Q_A, Q_B, { mod_inverse(Q_A, Q_B) }, { mod_inverse(Q_B, Q_A) }>;

    type RingCRTEval = IntModCycloCRTEval<
        2048,
        Q_A,
        Q_B,
        { mod_inverse(Q_A, Q_B) },
        { mod_inverse(Q_B, Q_A) },
        { find_sqrt_primitive_root(D, Q_A) },
        { find_sqrt_primitive_root(D, Q_B) },
    >;

    c.bench_function("math::Matrix<IntModCycloCRT> zero", |b| {
        type M = Matrix<2, 2, RingCRT>;
        b.iter(|| M::zero());
    });

    c.bench_function("math::Matrix<IntModCycloCRTEval> zero", |b| {
        type M = Matrix<2, 2, RingCRTEval>;
        b.iter(|| M::zero());
    });

    c.bench_function("math::Matrix<IntModCycloCRT> 2x2 add", |b| {
        type M = Matrix<2, 2, RingCRT>;
        let mut rng = ChaCha20Rng::from_entropy();
        let m1 = M::rand_uniform(&mut rng);
        let m2 = M::rand_uniform(&mut rng);

        b.iter(|| black_box(&m1) + black_box(&m2));
    });

    c.bench_function("math::Matrix<IntModCycloCRTEval> 2x2 add", |b| {
        type M = Matrix<2, 2, RingCRTEval>;
        let mut rng = ChaCha20Rng::from_entropy();
        let m1 = M::rand_uniform(&mut rng);
        let m2 = M::rand_uniform(&mut rng);

        b.iter(|| black_box(&m1) + black_box(&m2));
    });

    c.bench_function("math::Matrix<IntModCycloCRTEval> 2x2 mul", |b| {
        type M = Matrix<2, 2, RingCRTEval>;
        let mut rng = ChaCha20Rng::from_entropy();
        let m1 = M::rand_uniform(&mut rng);
        let m2 = M::rand_uniform(&mut rng);

        b.iter(|| black_box(&m1) * black_box(&m2));
    });

    c.bench_function("math::gadget inverse IntModCycloCRT 2x2, T = 8", |b| {
        type M = Matrix<2, 2, RingCRT>;
        const T: usize = 8;
        const Z: u64 = base_from_len(T, Q);
        let mut rng = ChaCha20Rng::from_entropy();
        let m1 = M::rand_uniform(&mut rng);

        b.iter(|| gadget_inverse::<_, 2, { 2 * T }, 2, Z, T>(black_box(&m1)));
    });

    c.bench_function("math::gadget inverse IntModCycloCRTEval 2x2, T = 8", |b| {
        type M = Matrix<2, 2, RingCRTEval>;
        const T: usize = 8;
        const Z: u64 = base_from_len(T, Q);
        let mut rng = ChaCha20Rng::from_entropy();
        let m1 = M::rand_uniform(&mut rng);

        b.iter(|| gadget_inverse::<_, 2, { 2 * T }, 2, Z, T>(black_box(&m1)));
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
