//! `cargo bench -p fm-match --bench instructions` — instruction count of a
//! full 90-minute match in LOD Abstract (`tick_logic` only), measured with
//! valgrind's callgrind. Deterministic, unlike wall-clock time on shared CI
//! runners (SPEC Fase 5: the same code measured 41.6 and 52.9 ms on two
//! runners). This is the fine-grained checkpoint of (c1):
//!
//! - fails if the count rose more than 1.5% over the committed baseline
//!   (`golden/instructions.txt`, the last accepted item);
//! - `UPDATE_INSTRUCTIONS=1` rewrites the baseline (only when accepting an
//!   item, like `UPDATE_GOLDEN`).
//!
//! Ruler: 560M instructions (whole program, demo seed 1) ≈ 41.2 ms criterion
//! on the CI runner of run 30 (commit 416e2a5); see SPEC for the conversion.
//!
//! The binary re-runs itself under callgrind twice — engine set-up only, and
//! set-up plus the match — and subtracts, so only `tick_logic` is counted.

use std::path::Path;
use std::process::Command;

const SEED: u64 = 2026;
const MAX_RISE: f64 = 0.015;
const MODE_ENV: &str = "FM_INSTRUCTIONS_CHILD";
const BASELINE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/golden/instructions.txt");

fn child(mode: &str) {
    let (db, setup) = fm_match::demo::demo_match(SEED);
    let mut engine = fm_match::MatchEngine::new(&setup, &db);
    if mode == "match" {
        while !engine.is_finished() {
            engine.tick_logic();
        }
    }
    // Keep the result observable so nothing is optimised away.
    println!("{}", engine.events().len());
}

/// Total `Ir` of one callgrind run of this binary in `mode`.
fn instructions(mode: &str, out_dir: &Path) -> u64 {
    let exe = std::env::current_exe().expect("own path");
    let out = out_dir.join(format!("callgrind.{mode}.out"));
    let status = Command::new("valgrind")
        .arg("--tool=callgrind")
        .arg(format!("--callgrind-out-file={}", out.display()))
        .arg(&exe)
        .env(MODE_ENV, mode)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("valgrind is installed (apt-get install valgrind)");
    assert!(status.success(), "callgrind run failed: {status}");
    let text = std::fs::read_to_string(&out).expect("callgrind output");
    text.lines()
        .find_map(|l| {
            l.strip_prefix("summary:")
                .or_else(|| l.strip_prefix("totals:"))
        })
        .and_then(|v| v.split_whitespace().next())
        .and_then(|v| v.parse().ok())
        .expect("callgrind summary line")
}

fn main() {
    if let Ok(mode) = std::env::var(MODE_ENV) {
        child(&mode);
        return;
    }
    let dir = std::env::temp_dir().join("fm-instructions");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let setup = instructions("setup", &dir);
    let full = instructions("match", &dir);
    let ticks = full - setup;
    println!("tick_logic full match (Abstract, seed {SEED}): {ticks} instructions");

    if std::env::var_os("UPDATE_INSTRUCTIONS").is_some() {
        std::fs::write(BASELINE, format!("{ticks}\n")).expect("write baseline");
        println!("baseline updated");
        return;
    }
    let baseline: u64 = std::fs::read_to_string(BASELINE)
        .expect("baseline file")
        .trim()
        .parse()
        .expect("baseline number");
    #[allow(clippy::cast_precision_loss)] // counts < 2^53
    let rise = ticks as f64 / baseline as f64 - 1.0;
    println!("baseline {baseline}: {:+.2}%", rise * 100.0);
    assert!(
        rise <= MAX_RISE,
        "instructions rose {:+.2}% over the baseline (limit +{:.1}%)",
        rise * 100.0,
        MAX_RISE * 100.0
    );
}
