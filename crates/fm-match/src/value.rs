//! Common value currency (spec Fase 5 (c1), item 2): every carrier option is
//! worth "probability that this possession ends in a goal". Positions are
//! valued with Expected Threat (xT); shots with their xG.
//!
//! The xT grid is Karun Singh's open 12×8 Expected Threat table (Premier
//! League event data), used as fixed data. It is *not* derived from this
//! engine — that would be circular — and may not transfer perfectly; it is
//! revisited in (c2).

use fm_core::{pitch, GoalEnd, Vec2};

/// Columns along the attacking direction (own goal → opponents' goal).
pub const XT_COLS: usize = 12;
/// Rows across the pitch width.
pub const XT_ROWS: usize = 8;

/// Karun Singh's open xT grid, `[row][col]`, col 0 at the attacker's own
/// goal. Symmetric across the width.
pub const KARUN_SINGH_XT: [[f32; XT_COLS]; XT_ROWS] = [
    [
        0.006_383_03,
        0.007_796_16,
        0.008_448_54,
        0.009_776_59,
        0.011_262_67,
        0.012_483_44,
        0.014_735_96,
        0.017_450_6,
        0.021_221_29,
        0.027_563_12,
        0.034_850_72,
        0.037_925_9,
    ],
    [
        0.007_500_72,
        0.008_785_89,
        0.009_423_82,
        0.010_594_9,
        0.012_147_19,
        0.013_845_4,
        0.016_118_13,
        0.018_703_47,
        0.024_015_21,
        0.029_532_72,
        0.040_669_92,
        0.046_477_21,
    ],
    [
        0.008_879_9,
        0.009_777_45,
        0.010_013_04,
        0.011_104_62,
        0.012_691_74,
        0.014_291_28,
        0.016_855_96,
        0.019_351_32,
        0.024_122_4,
        0.028_552_02,
        0.054_911_38,
        0.064_425_95,
    ],
    [
        0.009_410_56,
        0.010_827_22,
        0.010_165_49,
        0.011_323_76,
        0.012_626_46,
        0.014_845_98,
        0.016_895_28,
        0.019_970_7,
        0.023_851_49,
        0.035_113_26,
        0.108_051_02,
        0.257_453_62,
    ],
    [
        0.009_410_56,
        0.010_827_22,
        0.010_165_49,
        0.011_323_76,
        0.012_626_46,
        0.014_845_98,
        0.016_895_28,
        0.019_970_7,
        0.023_851_49,
        0.035_113_26,
        0.108_051_02,
        0.257_453_62,
    ],
    [
        0.008_879_9,
        0.009_777_45,
        0.010_013_04,
        0.011_104_62,
        0.012_691_74,
        0.014_291_28,
        0.016_855_96,
        0.019_351_32,
        0.024_122_4,
        0.028_552_02,
        0.054_911_38,
        0.064_425_95,
    ],
    [
        0.007_500_72,
        0.008_785_89,
        0.009_423_82,
        0.010_594_9,
        0.012_147_19,
        0.013_845_4,
        0.016_118_13,
        0.018_703_47,
        0.024_015_21,
        0.029_532_72,
        0.040_669_92,
        0.046_477_21,
    ],
    [
        0.006_383_03,
        0.007_796_16,
        0.008_448_54,
        0.009_776_59,
        0.011_262_67,
        0.012_483_44,
        0.014_735_96,
        0.017_450_6,
        0.021_221_29,
        0.027_563_12,
        0.034_850_72,
        0.037_925_9,
    ],
];

/// Fractional grid coordinate of `v` in `[0, len)` split into `n` cells,
/// measured between cell centres and clamped to the outer centres.
fn grid_coord(value: f32, len: f32, cells: usize) -> (usize, usize, f32) {
    #[allow(clippy::cast_precision_loss)] // cells ≤ 12
    let g = (value / len * cells as f32 - 0.5).clamp(0.0, (cells - 1) as f32);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 ≤ g < 12
    let lo = g as usize;
    let hi = (lo + 1).min(cells - 1);
    #[allow(clippy::cast_precision_loss)]
    let frac = g - lo as f32;
    (lo, hi, frac)
}

/// Expected Threat of having the ball at `pos` for the team attacking
/// `end`: bilinear between cell centres, so values change smoothly as the
/// ball moves (no jumps at cell borders that would flip decisions).
#[must_use]
pub fn xt(grid: &[[f32; XT_COLS]; XT_ROWS], pos: Vec2, end: GoalEnd) -> f32 {
    let along = match end {
        GoalEnd::Right => pos.x,
        GoalEnd::Left => pitch::LENGTH - pos.x,
    };
    let (c0, c1, fx) = grid_coord(along, pitch::LENGTH, XT_COLS);
    let (r0, r1, fy) = grid_coord(pos.y, pitch::WIDTH, XT_ROWS);
    let top = grid[r0][c0] + (grid[r0][c1] - grid[r0][c0]) * fx;
    let bottom = grid[r1][c0] + (grid[r1][c1] - grid[r1][c0]) * fx;
    top + (bottom - top) * fy
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: &[[f32; XT_COLS]; XT_ROWS] = &KARUN_SINGH_XT;

    #[test]
    fn cell_centres_return_the_table() {
        let cw = pitch::LENGTH / 12.0;
        let rh = pitch::WIDTH / 8.0;
        let p = Vec2::new(11.5 * cw, 3.5 * rh);
        assert!((xt(G, p, GoalEnd::Right) - 0.257_453_62).abs() < 1e-6);
        let p = Vec2::new(0.5 * cw, 0.5 * rh);
        assert!((xt(G, p, GoalEnd::Right) - 0.006_383_03).abs() < 1e-6);
    }

    #[test]
    fn threat_rises_toward_the_attacked_goal_and_mirrors() {
        let y = pitch::HALF_WIDTH;
        let mut prev = 0.0;
        for x in [10.0, 30.0, 50.0, 70.0, 90.0, 100.0] {
            let v = xt(G, Vec2::new(x, y), GoalEnd::Right);
            assert!(v > prev, "x={x}: {v} <= {prev}");
            prev = v;
            let mirrored = xt(G, Vec2::new(pitch::LENGTH - x, y), GoalEnd::Left);
            assert!((v - mirrored).abs() < 1e-7, "mirror at x={x}");
        }
        // Central beats wide at the same depth.
        let central = xt(G, Vec2::new(95.0, y), GoalEnd::Right);
        let wide = xt(G, Vec2::new(95.0, 5.0), GoalEnd::Right);
        assert!(central > wide);
    }
}
