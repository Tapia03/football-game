//! Structure-of-arrays player store and the weekly simulation step.

use fm_core::{rng_for_event, Rng};

use crate::player::{
    InjuryKind, PlayerDynamic, PlayerId, PlayerStatic, Position, BP_MAX, BP_NEUTRAL,
};

/// Domain separator for weekly streams, so a world seed that equals a match
/// seed never makes `weekly_update` reuse match draws.
const WEEKLY_DOMAIN: u64 = 0x5745_454B_4C59_0001; // "WEEKLY" tag

/// RNG stream for one player in one week. Independent of iteration order,
/// so a future parallel `weekly_update` produces the same result.
#[must_use]
pub fn rng_for_week(world_seed: u64, id: PlayerId, week: u32) -> Rng {
    rng_for_event(world_seed ^ WEEKLY_DOMAIN, id.0, week)
}

/// Two parallel columns indexed by `PlayerId`: cold `PlayerStatic` and hot
/// `PlayerDynamic`. `weekly_update` only streams the hot column plus the few
/// static fields it needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerDatabase {
    statics: Vec<PlayerStatic>,
    dynamics: Vec<PlayerDynamic>,
}

impl PlayerDatabase {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            statics: Vec::with_capacity(capacity),
            dynamics: Vec::with_capacity(capacity),
        }
    }

    /// Adds a player with fresh dynamic state and returns its id.
    ///
    /// # Panics
    /// If the database already holds `u32::MAX` players.
    pub fn create(&mut self, player: PlayerStatic) -> PlayerId {
        let id = PlayerId(u32::try_from(self.statics.len()).expect("PlayerId space exhausted"));
        self.statics.push(player);
        self.dynamics.push(PlayerDynamic::default());
        id
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.statics.len()
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.statics.is_empty()
    }

    /// # Panics
    /// If `id` was not created by this database.
    #[inline]
    #[must_use]
    pub fn static_of(&self, id: PlayerId) -> &PlayerStatic {
        &self.statics[id.index()]
    }

    /// # Panics
    /// If `id` was not created by this database.
    #[inline]
    pub fn static_mut(&mut self, id: PlayerId) -> &mut PlayerStatic {
        &mut self.statics[id.index()]
    }

    /// # Panics
    /// If `id` was not created by this database.
    #[inline]
    #[must_use]
    pub fn dynamic_of(&self, id: PlayerId) -> &PlayerDynamic {
        &self.dynamics[id.index()]
    }

    /// # Panics
    /// If `id` was not created by this database.
    #[inline]
    pub fn dynamic_mut(&mut self, id: PlayerId) -> &mut PlayerDynamic {
        &mut self.dynamics[id.index()]
    }

    #[must_use]
    pub fn statics(&self) -> &[PlayerStatic] {
        &self.statics
    }

    #[must_use]
    pub fn dynamics(&self) -> &[PlayerDynamic] {
        &self.dynamics
    }

    pub fn ids(&self) -> impl Iterator<Item = PlayerId> + '_ {
        // `create` refuses to grow past u32::MAX, so the zip never truncates.
        (0_u32..).zip(&self.statics).map(|(i, _)| PlayerId(i))
    }

    /// Adds match minutes to be consumed by the next `weekly_update`.
    ///
    /// # Panics
    /// If `id` was not created by this database.
    pub fn record_minutes(&mut self, id: PlayerId, minutes: u16) {
        let d = &mut self.dynamics[id.index()];
        d.minutes_this_week = d.minutes_this_week.saturating_add(minutes);
    }

    /// Advances fatigue, condition, form, injuries and morale of every player
    /// by one week. Allocation-free and deterministic in `(world_seed, week)`.
    pub fn weekly_update(&mut self, world_seed: u64, week: u32) {
        let rows = self.statics.iter().zip(self.dynamics.iter_mut());
        for (i, (s, d)) in (0_u32..).zip(rows) {
            let mut rng = rng_for_week(world_seed, PlayerId(i), week);
            update_player(s, d, &mut rng);
        }
    }
}

#[inline]
fn clamp_bp(v: i32) -> u16 {
    // Clamped to 0..=BP_MAX (10_000), so the narrowing cast is lossless.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let out = v.clamp(0, i32::from(BP_MAX)) as u16;
    out
}

/// Symmetric integer noise in `[-amp, amp]`.
#[inline]
fn noise(rng: &mut Rng, amp: i32) -> i32 {
    let amp = amp.max(0);
    #[allow(clippy::cast_sign_loss)] // amp >= 0
    let span = (2 * amp + 1) as u32;
    #[allow(clippy::cast_possible_wrap)] // span <= 2^31
    let draw = rng.below(span) as i32;
    draw - amp
}

/// One week for one player. Pure in `(s, d, rng)`; exposed for tests.
#[allow(clippy::similar_names)]
pub fn update_player(s: &PlayerStatic, d: &mut PlayerDynamic, rng: &mut Rng) {
    let phys = &s.attributes.physical;
    let hidden = &s.attributes.hidden;
    // Cap at ~3 full matches so a bad input cannot overflow the formulas.
    let minutes = i32::from(d.minutes_this_week.min(300));
    let bp_max = i32::from(BP_MAX);
    let neutral = i32::from(BP_NEUTRAL);

    // Keepers cover a fraction of an outfielder's distance (~5 km vs ~10-12
    // km per match) and sprint rarely, so per-minute load and injury
    // exposure are scaled down for them.
    let is_keeper = s.position == Position::Goalkeeper;

    // Fatigue: load ∝ minutes, damped by stamina (90 min, stamina 50 → 1800 bp);
    // recovery grows with natural fitness (nf 50 → 2250 bp/week), so one match
    // a week is sustainable and two are not. Keepers carry 40% of that load.
    let mut load = minutes * (130 - i32::from(phys.stamina)) / 4;
    if is_keeper {
        load = load * 2 / 5;
    }
    let recovery = 1_500 + i32::from(phys.natural_fitness) * 15;
    let fatigue = i32::from(d.fatigue) + load - recovery;
    d.fatigue = clamp_bp(fatigue);

    // Injury: an injured player heals one week at a time; a fit one rolls
    // p = (base 0.2% + minutes × proneness term) × (1 + fatigue), so tired,
    // injury-prone regulars get hurt most (≈1–2 injuries per season).
    if d.injury_weeks > 0 {
        d.injury_weeks -= 1;
        if d.injury_weeks == 0 {
            d.injury_kind = InjuryKind::None;
        }
    } else {
        // Keepers: half the per-minute exposure (fewer sprints and duels).
        let exposure = if is_keeper { 60 } else { 30 };
        let base = 20 + minutes * i32::from(hidden.injury_proneness) / exposure;
        let p = base * (bp_max + i32::from(d.fatigue)) / bp_max;
        #[allow(clippy::cast_possible_wrap)] // < 10_000
        let roll = rng.below(10_000) as i32;
        if roll < p {
            // Severity mix: half are knocks, 1 in 20 is long-term.
            #[allow(clippy::cast_possible_truncation)] // all ranges < 31
            let (kind, weeks) = match rng.below(100) {
                0..=49 => (InjuryKind::Knock, 1),
                50..=79 => (InjuryKind::Strain, 2 + rng.below(3) as u8),
                80..=94 => (InjuryKind::Tear, 4 + rng.below(7) as u8),
                _ => (InjuryKind::Major, 12 + rng.below(19) as u8),
            };
            d.injury_kind = kind;
            d.injury_weeks = weeks;
        }
    }

    // Condition chases (fully fit − fatigue) halfway each week; time out
    // injured costs match fitness, floored so a return is not hopeless.
    if d.is_injured() {
        d.condition = clamp_bp((i32::from(d.condition) - 800).max(4_000));
    } else {
        let target = bp_max - i32::from(d.fatigue);
        let c = i32::from(d.condition);
        d.condition = clamp_bp(c + (target - c) / 2);
    }

    // Morale: playing lifts it, sitting out for 3+ weeks hurts ambitious
    // players most; everything drifts 10% back to neutral each week.
    let mut morale = i32::from(d.morale);
    if d.is_injured() {
        morale -= 100;
        d.weeks_unused = 0;
    } else if minutes >= 45 {
        morale += 300;
        d.weeks_unused = 0;
    } else {
        d.weeks_unused = d.weeks_unused.saturating_add(1);
        if d.weeks_unused > 2 {
            morale -= i32::from(hidden.ambition) * 4;
        }
    }
    morale += (neutral - morale) / 10;
    d.morale = clamp_bp(morale);

    // Form chases a target set by playing time, morale and tiredness, plus
    // noise whose width shrinks with consistency (consistency 50 → ±720 bp).
    let playing = if minutes > 0 { 800 } else { -800 };
    let target = neutral + playing + (i32::from(d.morale) - neutral) / 4 - i32::from(d.fatigue) / 5;
    let f = i32::from(d.form);
    let amp = (110 - i32::from(hidden.consistency)) * 12;
    d.form = clamp_bp(f + (target - f) / 4 + noise(rng, amp));

    d.minutes_this_week = 0;
}

/// Order-sensitive digest of all dynamic state, for cross-target parity
/// checks (native vs WASM) without shipping the whole table.
#[must_use]
pub fn dynamics_digest(db: &PlayerDatabase) -> u64 {
    db.dynamics().iter().fold(0_u64, |h, d| {
        let packed = u64::from(d.fatigue)
            | u64::from(d.condition) << 14
            | u64::from(d.form) << 28
            | u64::from(d.morale) << 42
            | u64::from(d.injury_weeks) << 56;
        fm_core::splitmix64(h ^ packed ^ (d.injury_kind as u64) << 62)
    })
}

#[cfg(test)]
mod tests {
    use super::{rng_for_week, update_player, PlayerDatabase};
    use crate::generate::generate_database;
    use crate::player::{InjuryKind, PlayerDynamic, PlayerId, PlayerStatic, Position, BP_NEUTRAL};

    const SEED: u64 = 0xC0FF_EE00;

    /// Plays every player `minutes` per week for `weeks` weeks.
    fn run(db: &mut PlayerDatabase, seed: u64, weeks: u32, minutes: u16) {
        for w in 0..weeks {
            let ids: Vec<PlayerId> = db.ids().collect();
            for id in ids {
                if !db.dynamic_of(id).is_injured() {
                    db.record_minutes(id, minutes);
                }
            }
            db.weekly_update(seed, w);
        }
    }

    /// Counts new injuries over a season for players built by `tweak`.
    fn injuries_per_player(n: u32, weeks: u32, tweak: impl Fn(&mut PlayerStatic)) -> f64 {
        let mut db = generate_database(n, 1, 2026);
        let ids: Vec<PlayerId> = db.ids().collect();
        for &id in &ids {
            tweak(db.static_mut(id));
        }
        let mut injuries = 0_u32;
        for w in 0..weeks {
            for &id in &ids {
                if !db.dynamic_of(id).is_injured() {
                    db.record_minutes(id, 90);
                }
            }
            let before: Vec<bool> = ids
                .iter()
                .map(|&id| db.dynamic_of(id).is_injured())
                .collect();
            db.weekly_update(SEED, w);
            for (&id, was) in ids.iter().zip(before) {
                if !was && db.dynamic_of(id).is_injured() {
                    injuries += 1;
                }
            }
        }
        f64::from(injuries) / f64::from(n)
    }

    #[test]
    fn digest_matches_golden() {
        // Pinned natively; `fm-wasm/tests/web.rs` asserts the same digest in WASM.
        let mut db = generate_database(5_000, 12, 2026);
        for w in 0..30 {
            for i in (0..5_000).step_by(2) {
                db.record_minutes(PlayerId(i), 90);
            }
            db.weekly_update(99, w);
        }
        assert_eq!(super::dynamics_digest(&db), 0x71E5_90E3_714D_2DD4);
    }

    #[test]
    fn keepers_tire_less_and_get_hurt_less() {
        let make = |pos: Position| {
            let mut db = generate_database(3_000, 21, 2026);
            let ids: Vec<PlayerId> = db.ids().collect();
            for &id in &ids {
                let s = db.static_mut(id);
                s.position = pos;
                s.attributes.physical.stamina = 50;
                s.attributes.physical.natural_fitness = 30;
                s.attributes.hidden.injury_proneness = 60;
            }
            let mut fatigue = 0_u64;
            let mut injuries = 0_u32;
            for w in 0..38 {
                for &id in &ids {
                    if !db.dynamic_of(id).is_injured() {
                        db.record_minutes(id, 180);
                    }
                }
                let before: Vec<bool> = ids
                    .iter()
                    .map(|&id| db.dynamic_of(id).is_injured())
                    .collect();
                db.weekly_update(SEED, w);
                for (&id, was) in ids.iter().zip(before) {
                    let d = db.dynamic_of(id);
                    fatigue += u64::from(d.fatigue);
                    injuries += u32::from(!was && d.is_injured());
                }
            }
            (fatigue, injuries)
        };
        let (gk_fatigue, gk_inj) = make(Position::Goalkeeper);
        let (cb_fatigue, cb_inj) = make(Position::CentreBack);
        assert!(
            gk_fatigue * 2 < cb_fatigue,
            "gk {gk_fatigue} vs cb {cb_fatigue}"
        );
        assert!(gk_inj < cb_inj, "gk {gk_inj} vs cb {cb_inj}");
    }

    #[test]
    fn create_and_lookup() {
        let template = *generate_database(1, 3, 2026).static_of(PlayerId(0));
        let mut db = PlayerDatabase::new();
        assert!(db.is_empty());
        let a = db.create(template);
        let b = db.create(template);
        assert_eq!((a, b), (PlayerId(0), PlayerId(1)));
        assert_eq!(db.len(), 2);
        assert_eq!(db.static_of(b), &template);
        assert_eq!(db.dynamic_of(a), &PlayerDynamic::default());
        db.record_minutes(a, 90);
        db.record_minutes(a, 30);
        assert_eq!(db.dynamic_of(a).minutes_this_week, 120);
        db.dynamic_mut(b).morale = 7_000;
        assert_eq!(db.dynamics()[1].morale, 7_000);
        assert_eq!(db.ids().collect::<Vec<_>>(), vec![a, b]);
    }

    #[test]
    fn weekly_update_is_deterministic() {
        let mut a = generate_database(2_000, 9, 2026);
        let mut b = generate_database(2_000, 9, 2026);
        run(&mut a, SEED, 52, 90);
        run(&mut b, SEED, 52, 90);
        assert_eq!(a, b);

        let mut c = generate_database(2_000, 9, 2026);
        run(&mut c, SEED + 1, 52, 90);
        assert_ne!(a.dynamics(), c.dynamics(), "world seed must matter");
    }

    #[test]
    fn per_player_stream_is_order_independent() {
        // The database result for player k equals updating k alone with its
        // own stream: no hidden coupling between players.
        let mut db = generate_database(500, 4, 2026);
        let mut lone = *db.dynamic_of(PlayerId(321));
        db.record_minutes(PlayerId(321), 90);
        lone.minutes_this_week = 90;
        db.weekly_update(SEED, 7);
        let s = *db.static_of(PlayerId(321));
        update_player(&s, &mut lone, &mut rng_for_week(SEED, PlayerId(321), 7));
        assert_eq!(&lone, db.dynamic_of(PlayerId(321)));
    }

    #[test]
    fn invariants_hold_over_ten_seasons() {
        let mut db = generate_database(3_000, 5, 2026);
        for w in 0..520_u32 {
            let ids: Vec<PlayerId> = db.ids().collect();
            for id in ids {
                // Mixed workloads, including absurd ones (300+ minutes).
                let minutes = u16::try_from((id.0 * 37 + w * 11) % 400).unwrap();
                db.record_minutes(id, minutes);
            }
            db.weekly_update(SEED, w);
            for d in db.dynamics() {
                assert!(d.is_valid(), "invalid state at week {w}: {d:?}");
                assert_eq!(d.minutes_this_week, 0);
                assert!(d.injury_weeks <= 30);
            }
        }
    }

    #[test]
    fn fatigue_rises_with_congestion_and_recovers_with_rest() {
        let mut db = generate_database(200, 6, 2026);
        run(&mut db, SEED, 6, 270);
        let tired: u32 = db.dynamics().iter().map(|d| u32::from(d.fatigue)).sum();
        assert!(tired > 0, "three matches a week must accumulate fatigue");
        run(&mut db, SEED, 6, 0);
        assert!(
            db.dynamics().iter().all(|d| d.fatigue == 0),
            "rest clears fatigue"
        );
        // Players just back from injury are still regaining condition (it
        // closes half the gap per week), so check the squad average.
        let fit: Vec<u32> = db
            .dynamics()
            .iter()
            .filter(|d| !d.is_injured())
            .map(|d| u32::from(d.condition))
            .collect();
        let avg = fit.iter().sum::<u32>() / u32::try_from(fit.len()).unwrap();
        assert!(avg > 9_000, "average condition after rest = {avg}");
    }

    #[test]
    fn injury_rate_is_plausible_and_follows_proneness() {
        let typical = injuries_per_player(4_000, 38, |s| s.attributes.hidden.injury_proneness = 50);
        assert!(
            (0.5..3.0).contains(&typical),
            "injuries/player/season = {typical}"
        );

        let fragile = injuries_per_player(4_000, 38, |s| s.attributes.hidden.injury_proneness = 95);
        let robust = injuries_per_player(4_000, 38, |s| s.attributes.hidden.injury_proneness = 5);
        assert!(
            fragile > robust * 2.0,
            "fragile {fragile} vs robust {robust}"
        );
    }

    #[test]
    fn injuries_heal_and_carry_a_kind() {
        let mut db = generate_database(3_000, 8, 2026);
        run(&mut db, SEED, 20, 180);
        let injured = db.dynamics().iter().filter(|d| d.is_injured()).count();
        assert!(injured > 0);
        assert!(db
            .dynamics()
            .iter()
            .filter(|d| d.is_injured())
            .all(|d| d.injury_kind != InjuryKind::None));
        // Longest injury is 30 weeks; after 31 weeks of rest everyone is back
        // (resting players still roll the small base risk, so allow a few).
        for w in 20..51 {
            db.weekly_update(SEED, w);
        }
        let still = db.dynamics().iter().filter(|d| d.injury_weeks > 4).count();
        assert!(
            still * 100 < db.len(),
            "{still} players still long-term injured"
        );
    }

    #[test]
    fn morale_tracks_playing_time() {
        let mut playing = generate_database(500, 10, 2026);
        run(&mut playing, SEED, 10, 90);
        let mut benched = generate_database(500, 10, 2026);
        run(&mut benched, SEED, 10, 0);
        let mean = |db: &PlayerDatabase| {
            db.dynamics()
                .iter()
                .map(|d| f64::from(d.morale))
                .sum::<f64>()
                / f64::from(u32::try_from(db.len()).unwrap())
        };
        assert!(mean(&playing) > f64::from(BP_NEUTRAL));
        assert!(mean(&benched) < f64::from(BP_NEUTRAL));
    }

    #[test]
    fn consistency_narrows_form_swings() {
        let swing = |consistency: u8| {
            let mut db = generate_database(300, 11, 2026);
            let ids: Vec<PlayerId> = db.ids().collect();
            for &id in &ids {
                db.static_mut(id).attributes.hidden.consistency = consistency;
            }
            let mut total = 0_u64;
            for w in 0..40 {
                let before: Vec<u16> = db.dynamics().iter().map(|d| d.form).collect();
                for &id in &ids {
                    db.record_minutes(id, 90);
                }
                db.weekly_update(SEED, w);
                total += db
                    .dynamics()
                    .iter()
                    .zip(before)
                    .map(|(d, b)| u64::from(d.form.abs_diff(b)))
                    .sum::<u64>();
            }
            total
        };
        assert!(swing(100) * 2 < swing(10));
    }
}
