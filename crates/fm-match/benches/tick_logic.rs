//! `cargo bench -p fm-match --bench tick_logic` — cost of a full 90-minute
//! match in LOD Abstract (`tick_logic` only). Target ≤ 40 ms on the CI runner
//! (SPEC §0.17); the hard gate (50 ms) is `tests/perf.rs`.

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use criterion::{criterion_group, BatchSize, Criterion};
    use fm_match::demo::demo_match;
    use fm_match::MatchEngine;

    fn full_match_abstract(c: &mut Criterion) {
        let (db, setup) = demo_match(2026);
        let mut group = c.benchmark_group("tick_logic");
        group.sample_size(20);
        group.bench_function("full_match_abstract", |b| {
            b.iter_batched_ref(
                || MatchEngine::new(&setup, &db),
                |engine| {
                    while !engine.is_finished() {
                        engine.tick_logic();
                    }
                },
                BatchSize::LargeInput,
            );
        });
        group.finish();
    }

    criterion_group!(benches, full_match_abstract);
    pub use benches as run;
}

#[cfg(not(target_arch = "wasm32"))]
criterion::criterion_main!(native::run);

#[cfg(target_arch = "wasm32")]
fn main() {}
