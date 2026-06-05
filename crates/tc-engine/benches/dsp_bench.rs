use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_equalizer(c: &mut Criterion) {
    use tc_engine::dsp::equalizer::ParametricEq;
    let mut eq = ParametricEq::default_10_band(44100.0);
    eq.set_enabled(true);
    c.bench_function("equalizer/10_band_stereo_frame", |b| {
        b.iter(|| {
            black_box(eq.process(black_box(0.5_f64), black_box(0.3_f64)))
        });
    });
    c.bench_function("equalizer/10_band_stereo_block_256", |b| {
        let mut frames: Vec<(f64, f64)> = vec![(0.5, 0.3); 256];
        b.iter(|| {
            for (l, r) in frames.iter_mut() {
                (*l, *r) = eq.process(*l, *r);
            }
            black_box(&frames);
        });
    });
}

fn bench_limiter(c: &mut Criterion) {
    use tc_engine::dsp::limiter::LookaheadLimiter;
    let mut limiter = LookaheadLimiter::new_with_params(44100.0, 5.0, 1.0, 100.0, -1.0, true);
    limiter.set_enabled(true);
    c.bench_function("limiter/lookahead_stereo_frame", |b| {
        b.iter(|| black_box(limiter.process(black_box(0.9_f64), black_box(0.9_f64))))
    });
}

fn bench_loudness(c: &mut Criterion) {
    use tc_engine::dsp::loudness::{LoudnessMode, LoudnessNormalizer};
    let mut norm = LoudnessNormalizer::new(44100.0);
    norm.set_mode(LoudnessMode::EbuR128);
    c.bench_function("loudness/ebu_r128_stereo_frame", |b| {
        b.iter(|| black_box(norm.process(black_box(0.5_f64), black_box(0.5_f64))))
    });
}

criterion_group!(benches, bench_equalizer, bench_limiter, bench_loudness);
criterion_main!(benches);

