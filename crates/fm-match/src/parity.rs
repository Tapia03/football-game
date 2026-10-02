//! Native ↔ WASM parity for the whole engine (`test_libm_parity_in_engine`).
//!
//! `report()` plays a full demo match and serialises, as raw bits: one
//! complete snapshot every `SNAPSHOT_EVERY` ticks, every event, the final
//! score, and a digest of a snapshot at *every* tick. The native test and the
//! `wasm-bindgen-test` both compare their own `report()` with the committed
//! golden, field by field, so any ulp of divergence fails with its location.
//!
//! Regenerate (only for an intentional engine change):
//! `UPDATE_GOLDEN=1 cargo test -p fm-match test_libm_parity_in_engine`.

use core::fmt::Write as _;

use crate::demo::demo_match;
use crate::engine::MatchEngine;
use crate::snapshot::{LodLevel, MatchSnapshot};

/// Seed of the parity match.
pub const PARITY_SEED: u64 = 0x5E_ED0F_FA17;
/// One full snapshot every 30 s of match time → 180 snapshots.
pub const SNAPSHOT_EVERY: u32 = 300;

/// Committed golden produced natively.
pub const GOLDEN: &str = include_str!("../golden/engine_parity.txt");

fn snapshot_line(out: &mut String, s: &MatchSnapshot) {
    let _ = write!(
        out,
        "S tick={} t={} score={}-{} ball={:08x},{:08x},{:08x}",
        s.tick,
        s.t_ms,
        s.score[0],
        s.score[1],
        s.ball.x.to_bits(),
        s.ball.y.to_bits(),
        s.ball.z.to_bits()
    );
    for (i, p) in s.players.iter().enumerate() {
        let _ = write!(
            out,
            " p{i}={:08x},{:08x}{}",
            p.pos.x.to_bits(),
            p.pos.y.to_bits(),
            if p.sent_off { "x" } else { "" }
        );
    }
    out.push('\n');
}

fn fold(h: u64, s: &MatchSnapshot) -> u64 {
    let mut h = fm_core::splitmix64(h ^ u64::from(s.tick));
    h = fm_core::splitmix64(
        h ^ u64::from(s.ball.x.to_bits()) ^ (u64::from(s.ball.y.to_bits()) << 32),
    );
    h = fm_core::splitmix64(h ^ u64::from(s.ball.z.to_bits()));
    for p in &s.players {
        h = fm_core::splitmix64(
            h ^ u64::from(p.pos.x.to_bits()) ^ (u64::from(p.pos.y.to_bits()) << 32),
        );
    }
    h
}

/// Plays the parity match and serialises it (see module docs).
#[must_use]
pub fn report() -> String {
    let (db, setup) = demo_match(PARITY_SEED);
    let mut engine = MatchEngine::new(&setup, &db);
    let mut out = String::new();
    let mut digest = 0_u64;
    while !engine.is_finished() {
        engine.tick_logic();
        let now = engine.state().now_ms();
        if let Some(snap) = engine.sample(LodLevel::Reduced, now) {
            digest = fold(digest, &snap);
            if snap.tick % SNAPSHOT_EVERY == 0 {
                snapshot_line(&mut out, &snap);
            }
        }
    }
    for e in engine.events() {
        let _ = writeln!(out, "E tick={} {:?}", e.tick, e.kind);
    }
    let st = engine.state();
    let _ = writeln!(out, "F score={}-{}", st.teams[0].score, st.teams[1].score);
    let _ = writeln!(out, "D digest={digest:016x}");
    out
}

/// First difference between `actual` and `golden`, as
/// `(line number, field index, expected field, actual field)`.
#[must_use]
pub fn first_mismatch(golden: &str, actual: &str) -> Option<(usize, usize, String, String)> {
    let mut g = golden.lines();
    let mut a = actual.lines();
    let mut line = 0;
    loop {
        line += 1;
        match (g.next(), a.next()) {
            (None, None) => return None,
            (Some(gl), Some(al)) if gl == al => {}
            (gl, al) => {
                let gl = gl.unwrap_or("<eof>");
                let al = al.unwrap_or("<eof>");
                let mut gf = gl.split(' ');
                let mut af = al.split(' ');
                let mut field = 0;
                loop {
                    match (gf.next(), af.next()) {
                        (Some(x), Some(y)) if x == y => field += 1,
                        (x, y) => {
                            return Some((
                                line,
                                field,
                                x.unwrap_or("<none>").to_owned(),
                                y.unwrap_or("<none>").to_owned(),
                            ))
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{first_mismatch, report, GOLDEN};

    #[test]
    fn test_libm_parity_in_engine() {
        let actual = report();
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            let path = concat!(env!("CARGO_MANIFEST_DIR"), "/golden/engine_parity.txt");
            std::fs::write(path, &actual).expect("write golden");
            return;
        }
        if let Some((line, field, expected, got)) = first_mismatch(GOLDEN, &actual) {
            panic!("engine diverges from golden at line {line}, field {field}: expected `{expected}`, got `{got}`");
        }
    }

    #[test]
    fn mismatch_reports_field() {
        assert_eq!(first_mismatch("a b\nc d\n", "a b\nc d\n"), None);
        assert_eq!(
            first_mismatch("S x=1 y=2\n", "S x=1 y=3\n"),
            Some((1, 2, "y=2".into(), "y=3".into()))
        );
        assert_eq!(
            first_mismatch("a\n", "a\nb\n"),
            Some((2, 0, "<eof>".into(), "b".into()))
        );
    }
}
