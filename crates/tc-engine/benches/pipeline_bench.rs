use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use tc_config::EngineConfig;
use tc_engine::dsp::pipeline::DspPipeline;

fn bench_pipeline(c: &mut Criterion) {
    let config = EngineConfig::default();
    let mut pipeline = DspPipeline::from_config(&config, 44100.0);

    let mut group = c.benchmark_group("pipeline");

    // Single frame throughput
    group.bench_function("full_chain/single_frame", |b| {
        b.iter(|| black_box(pipeline.process(black_box(0.5_f32), black_box(0.3_f32))))
    });

    // Block throughput (realistic audio callback size)
    const BLOCK: usize = 512;
    group.throughput(Throughput::Elements(BLOCK as u64));
    group.bench_function("full_chain/block_512", |b| {
        let frames: [(f32, f32); BLOCK] = [(0.5, 0.3); BLOCK];
        b.iter(|| {
            // DspPipeline::process is the per-sample API; loop it for a
            // realistic block benchmark. (Earlier versions exposed a
            // `process_batch` helper, but it was removed when the DSP
            // pipeline was refactored to process one stereo sample at a
            // time. The loop is the correct comparison point.)
            let mut last = (0.0_f32, 0.0_f32);
            for &(l, r) in &frames {
                last = pipeline.process(black_box(l), black_box(r));
            }
            black_box(last);
        });
    });

    // Larger block simulating high-latency scenario
    const BLOCK_4K: usize = 4096;
    group.throughput(Throughput::Elements(BLOCK_4K as u64));
    group.bench_function("full_chain/block_4096", |b| {
        let frames: Vec<(f32, f32)> = vec![(0.5, 0.3); BLOCK_4K];
        b.iter(|| {
            let mut last = (0.0_f32, 0.0_f32);
            for &(l, r) in &frames {
                last = pipeline.process(black_box(l), black_box(r));
            }
            black_box(last);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_pipeline);
criterion_main!(benches);
