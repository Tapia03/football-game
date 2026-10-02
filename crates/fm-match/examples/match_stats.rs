//! `cargo run --release -p fm-match --example match_stats` — average event
//! counts over N demo matches, used to calibrate the engine (see SPEC Fase 5).
use fm_match::{demo::demo_match, EventKind, LodLevel, MatchEngine, RestartKind};
use std::collections::BTreeMap;
fn main() {
    let n = 60u32;
    let mut agg: BTreeMap<String, f64> = BTreeMap::new();
    let t0 = std::time::Instant::now();
    for seed in 0..u64::from(n) {
        let (db, setup) = demo_match(seed);
        let mut e = MatchEngine::new(&setup, &db);
        e.run(LodLevel::Abstract, |_| {});
        let mut add = |k: &str| *agg.entry(k.to_string()).or_default() += 1.0 / f64::from(n);
        for ev in e.events() {
            match ev.kind {
                EventKind::Goal { .. } => add("goals"),
                EventKind::Shot { on_target, .. } => {
                    add("shots");
                    if on_target {
                        add("shots_on");
                    }
                }
                EventKind::Save { .. } => add("saves"),
                EventKind::Foul { .. } => add("fouls"),
                EventKind::Card {
                    card: fm_match::CardKind::Yellow,
                    ..
                } => add("yellow"),
                EventKind::Card { .. } => add("red"),
                EventKind::Restart { kind, .. } => add(match kind {
                    RestartKind::KickOff => "r_kickoff",
                    RestartKind::ThrowIn => "r_throw",
                    RestartKind::GoalKick => "r_goalkick",
                    RestartKind::Corner => "r_corner",
                    RestartKind::FreeKick => "r_fk",
                    RestartKind::Penalty => "r_pen",
                }),
                _ => {}
            }
        }
        let s = e.state();
        *agg.entry("passes".into()).or_default() +=
            f64::from(s.teams[0].passes + s.teams[1].passes) / f64::from(n);
        *agg.entry("passes_ok".into()).or_default() +=
            f64::from(s.teams[0].passes_completed + s.teams[1].passes_completed) / f64::from(n);
        *agg.entry("tackles".into()).or_default() +=
            f64::from(s.teams[0].tackles + s.teams[1].tackles) / f64::from(n);
        let tackles: u32 = s.players.iter().map(|p| p.action_count).sum();
        *agg.entry("actions".into()).or_default() += f64::from(tackles) / f64::from(n);
    }
    for (k, v) in &agg {
        println!("{k:>11}: {v:.2}");
    }
    println!("{:?}/match", t0.elapsed() / n);
}
