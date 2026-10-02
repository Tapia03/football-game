//! `cargo bench -p fm-entities` — informative numbers for the phase report.
//! The hard gate (< 150 ms for 500k) is `tests/perf.rs`.

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use criterion::{criterion_group, BatchSize, Criterion};
    use fm_entities::{generate_database, PlayerId};

    fn weekly_update(c: &mut Criterion) {
        let mut group = c.benchmark_group("weekly_update");
        group.sample_size(20);
        for n in [50_000_u32, 500_000] {
            let base = generate_database(n, 2026, 2026);
            group.bench_function(format!("{n}_players"), |b| {
                b.iter_batched_ref(
                    || {
                        let mut db = base.clone();
                        for i in (0..n).step_by(2) {
                            db.record_minutes(PlayerId(i), 90);
                        }
                        db
                    },
                    |db| db.weekly_update(1, 0),
                    BatchSize::LargeInput,
                );
            });
        }
        group.finish();
    }

    fn generate(c: &mut Criterion) {
        let mut group = c.benchmark_group("generate_database");
        group.sample_size(10);
        group.bench_function("500000_players", |b| {
            b.iter(|| generate_database(500_000, 2026, 2026));
        });
        group.finish();
    }

    criterion_group!(benches, weekly_update, generate);
    pub use benches as run;
}

#[cfg(not(target_arch = "wasm32"))]
criterion::criterion_main!(native::run);

#[cfg(target_arch = "wasm32")]
fn main() {}
