//! Golden vectors for libm parity between native and WASM.
//!
//! `generate()` evaluates `sinf`, `cosf`, `powf`, `sqrtf`, `expf`, `logf` on a
//! fixed input set and serialises each result as raw bits. The output is
//! committed in `golden/libm_f32.txt`; native tests and `wasm-bindgen-test`
//! both compare against that file, so any drift between targets fails CI.
//!
//! Regenerate with `UPDATE_GOLDEN=1 cargo test -p fm-core libm_golden`.

use core::fmt::Write as _;

/// Committed golden file, embedded so the WASM test needs no filesystem.
pub const GOLDEN: &str = include_str!("../golden/libm_f32.txt");

/// Deterministic input grid. Built from integer steps (not accumulated floats)
/// so the inputs themselves are identical on every target.
fn inputs() -> impl Iterator<Item = f32> {
    // -40.0 ..= 40.0 in steps of 0.25: covers several periods for sin/cos and
    // the range used by ball physics (times, speeds, distances).
    (-160_i16..=160).map(|i| f32::from(i) * 0.25)
}

fn line(out: &mut String, name: &str, args: &[f32], result: f32) {
    out.push_str(name);
    for a in args {
        let _ = write!(out, " {:08x}", a.to_bits());
    }
    let _ = writeln!(out, " {:08x}", result.to_bits());
}

/// Evaluates every golden case and returns the serialised table.
#[must_use]
pub fn generate() -> String {
    let mut out = String::new();
    for x in inputs() {
        line(&mut out, "sinf", &[x], libm::sinf(x));
        line(&mut out, "cosf", &[x], libm::cosf(x));
        line(&mut out, "expf", &[x * 0.25], libm::expf(x * 0.25));
        let pos = x.abs() + 0.125;
        line(&mut out, "sqrtf", &[pos], libm::sqrtf(pos));
        line(&mut out, "logf", &[pos], libm::logf(pos));
        // Exponent range kept small so results stay finite and informative.
        let e = x * 0.1;
        line(&mut out, "powf", &[pos, e], libm::powf(pos, e));
    }
    out
}

/// Returns the first line that differs from the golden file, if any.
#[must_use]
pub fn first_mismatch() -> Option<(usize, String, String)> {
    let actual = generate();
    let mut golden_lines = GOLDEN.lines();
    for (i, a) in actual.lines().enumerate() {
        match golden_lines.next() {
            Some(g) if g == a => {}
            Some(g) => return Some((i + 1, g.to_owned(), a.to_owned())),
            None => return Some((i + 1, String::from("<eof>"), a.to_owned())),
        }
    }
    golden_lines.next().map(|g| {
        (
            actual.lines().count() + 1,
            g.to_owned(),
            String::from("<eof>"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{first_mismatch, generate};

    #[test]
    fn libm_golden_matches_native() {
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            let path = concat!(env!("CARGO_MANIFEST_DIR"), "/golden/libm_f32.txt");
            std::fs::write(path, generate()).expect("write golden");
            return;
        }
        if let Some((line, expected, actual)) = first_mismatch() {
            panic!("libm golden mismatch at line {line}: expected `{expected}`, got `{actual}`");
        }
    }

    #[test]
    fn generation_is_stable_within_a_process() {
        assert_eq!(generate(), generate());
    }
}
