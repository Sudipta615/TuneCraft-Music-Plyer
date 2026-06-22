use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_equalizer(c: &mut Criterion) {
    use tc_engine::dsp::equalizer::ParametricEq;
    let mut eq = ParametricEq::default_10_band(44100.0);
    eq.set_enabled(true);
    c.bench_function("equalizer/10_band_stereo_frame", |b| {
        b.iter(|| black_box(eq.process(black_box(0.5_f32), black_box(0.3_f32))));
    });
    c.bench_function("equalizer/10_band_stereo_block_256", |b| {
        let mut frames: Vec<(f32, f32)> = vec![(0.5, 0.3); 256];
        b.iter(|| {
            for (l, r) in frames.iter_mut() {
                let (out_l, out_r) = black_box(eq.process(black_box(*l), black_box(*r)));
                *l = out_l;
                *r = out_r;
            }
            black_box(&mut frames);
        });
    });
}

fn bench_limiter(c: &mut Criterion) {
    use tc_engine::dsp::limiter::LookaheadLimiter;
    let mut limiter = LookaheadLimiter::new_with_params(44100.0, 5.0, 1.0, 100.0, -1.0, true);
    limiter.set_enabled(true);
    c.bench_function("limiter/lookahead_stereo_frame", |b| {
        b.iter(|| black_box(limiter.process(black_box(0.9_f32), black_box(0.9_f32))))
    });
}

fn bench_loudness(c: &mut Criterion) {
    use tc_engine::dsp::loudness::{LoudnessMode, LoudnessNormalizer};
    let mut norm = LoudnessNormalizer::new(44100.0);
    norm.set_mode(LoudnessMode::EbuR128);
    c.bench_function("loudness/ebu_r128_stereo_frame", |b| {
        b.iter(|| black_box(norm.process(black_box(0.5_f32), black_box(0.5_f32))))
    });
}

/// v3.1.0: Spectrum analyzer benchmark. Measures the per-sample cost
/// of the FFT tap (most samples are no-ops due to the hop), and a
/// full FFT block (every 512 samples). Together these give us the
/// true amortized cost of the analyzer.
fn bench_spectrum(c: &mut Criterion) {
    use tc_engine::dsp::spectrum::SpectrumAnalyzer;
    let mut analyzer = SpectrumAnalyzer::new(44100.0);

    // Per-sample cost (no FFT runs — only 1 in 512 samples triggers it).
    // This is the cost added to every audio callback invocation.
    c.bench_function("spectrum/per_sample_no_fft", |b| {
        let mut i = 0;
        b.iter(|| {
            // Avoid hitting the hop boundary on every iteration —
            // criterion's `iter` calls us many times, but the analyzer
            // only FFTs every 512 samples, so most calls are no-ops.
            i = (i + 1) % 511;
            analyzer.process(black_box(0.5_f32), black_box(0.3_f32));
        });
    });

    // Amortized cost over a full hop (512 samples). This is the true
    // cost the audio engine pays per output frame.
    c.bench_function("spectrum/amortized_512_sample_hop", |b| {
        b.iter(|| {
            for _ in 0..512 {
                analyzer.process(black_box(0.5_f32), black_box(0.3_f32));
            }
            black_box(analyzer.snapshot());
        });
    });
}

criterion_group!(
    benches,
    bench_equalizer,
    bench_limiter,
    bench_loudness,
    bench_spectrum
);
criterion_main!(benches);
