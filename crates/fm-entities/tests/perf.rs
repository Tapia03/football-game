//! Acceptance criterion 15: `weekly_update` of 500k players in < 150 ms.
//! Meaningful only in release: run with
//! `cargo test --release -p fm-entities --test perf -- --ignored --nocapture`.

use std::time::{Duration, Instant};

use fm_entities::{generate_database, PlayerId};

#[test]
#[ignore = "timing gate; run in release (CI job `bench`)"]
fn weekly_update_500k_under_150ms() {
    const N: u32 = 500_000;
    let mut db = generate_database(N, 2026, 2026);
    let mut best = Duration::MAX;
    for week in 0..5 {
        for i in (0..N).step_by(2) {
            db.record_minutes(PlayerId(i), 90);
        }
        let t = Instant::now();
        db.weekly_update(1, week);
        best = best.min(t.elapsed());
    }
    println!("weekly_update 500k: best of 5 = {best:?}");
    assert!(best < Duration::from_millis(150), "too slow: {best:?}");
}
