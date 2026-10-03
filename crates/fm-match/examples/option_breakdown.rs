//! `cargo run --release -p fm-match --features diagnostics --example
//! option_breakdown` — why the best pass is worth three times less than
//! carrying (SPEC Fase 5): at every carrier decision, the parts of each
//! value as `choose_action` computes them:
//!
//! - pass    = success × xT(receiver) − (1 − success) × opponents' xT(receiver)
//! - dribble = keep × xT(5 m ahead)   − (1 − keep)    × opponents' xT(carrier)
//! - hold    = xT(carrier) × (1 − erosion × ticks on the ball)
//!
//! split by how the spell on the ball ends (pass by value, or pass at the
//! forced release). Observed from outside after each tick. No engine change.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::many_single_char_names
)]
use fm_match::decision::OptionValues;
use fm_match::state::{BallState, FlightIntent};
use fm_match::{DecisionSystem, Formation, MatchEngine, TickFrame};

#[derive(Default, Clone, Copy)]
struct Sum {
    n: u64,
    here: f64,
    hold: f64,
    // Pass.
    pass: f64,
    pass_gain: f64,
    pass_loss: f64,
    success: f64,
    survive: f64,
    pass_win: f64,
    pass_cost: f64,
    back: u64,
    // Dribble.
    dribble: f64,
    dribble_gain: f64,
    dribble_loss: f64,
    keep: f64,
    dribble_win: f64,
    dribble_cost: f64,
    cramped: u64,
    // The receiver's threat against the dribble target's.
    receiver_ahead: u64,
    // What the pass would be worth with the dribble's keep chance, and
    // with no loss term at all.
    pass_at_keep: f64,
    pass_beats_dribble: u64,
    pass_beats_dribble_at_keep: u64,
}

impl Sum {
    fn add(&mut self, v: &OptionValues) {
        self.n += 1;
        self.here += f64::from(v.here);
        self.hold += f64::from(v.hold);
        self.pass += f64::from(v.pass);
        self.pass_gain += f64::from(v.pass_gain);
        self.pass_loss += f64::from(v.pass_loss);
        self.success += f64::from(v.pass_success);
        self.survive += f64::from(v.pass_survive);
        self.pass_win += f64::from(v.pass_success * v.pass_gain);
        self.pass_cost += f64::from((1.0 - v.pass_success) * v.pass_loss);
        self.back += u64::from(v.pass_gain < v.here);
        self.dribble += f64::from(v.dribble);
        self.dribble_gain += f64::from(v.dribble_gain);
        self.dribble_loss += f64::from(v.dribble_loss);
        self.keep += f64::from(v.dribble_keep);
        self.dribble_win += f64::from(v.dribble_keep * v.dribble_gain);
        self.dribble_cost += f64::from((1.0 - v.dribble_keep) * v.dribble_loss);
        self.cramped += u64::from(v.dribble_keep < 0.9);
        self.receiver_ahead += u64::from(v.pass_gain > v.dribble_gain);
        let at_keep = v.dribble_keep * v.pass_gain - (1.0 - v.dribble_keep) * v.pass_loss;
        self.pass_at_keep += f64::from(at_keep);
        self.pass_beats_dribble += u64::from(v.pass > v.dribble);
        self.pass_beats_dribble_at_keep += u64::from(at_keep > v.dribble);
    }

    fn print(&self, label: &str, matches: u64) {
        let n = self.n.max(1) as f64;
        let k = |x: f64| 1000.0 * x / n;
        let p = |x: f64| 100.0 * x / n;
        println!(
            "  {label}: {:.0} decisions/match (values x1000)",
            self.n as f64 / matches as f64
        );
        println!(
            "    carrier: xT here {:.2} | hold {:.2}",
            k(self.here),
            k(self.hold)
        );
        println!(
            "    best pass {:.2} = success {:.0}% x xT receiver {:.2} [= {:.2}] - failure x opponents' xT there {:.2} [= {:.2}]",
            k(self.pass),
            p(self.success),
            k(self.pass_gain),
            k(self.pass_win),
            k(self.pass_loss),
            k(self.pass_cost)
        );
        println!(
            "      success = lane survival {:.0}% x accuracy and first touch {:.0}% ; the best pass goes to a lower xT than the carrier's in {:.0}%",
            p(self.survive),
            100.0 * self.success / self.survive.max(1e-9),
            p(self.back as f64)
        );
        println!(
            "    dribble   {:.2} = keep {:.0}% x xT 5 m ahead {:.2} [= {:.2}] - loss x opponents' xT here {:.2} [= {:.2}] ; cramped in {:.0}%",
            k(self.dribble),
            p(self.keep),
            k(self.dribble_gain),
            k(self.dribble_win),
            k(self.dribble_loss),
            k(self.dribble_cost),
            p(self.cramped as f64)
        );
        println!(
            "    the receiver's xT is above the dribble target's in {:.0}% ; pass > dribble in {:.0}% ; with the dribble's keep chance in place of the pass success it would be {:.2} and beat the dribble in {:.0}%",
            p(self.receiver_ahead as f64),
            p(self.pass_beats_dribble as f64),
            k(self.pass_at_keep),
            p(self.pass_beats_dribble_at_keep as f64)
        );
    }
}

fn main() {
    let n: u64 = std::env::var("FM_MATCHES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    for (name, home, away) in [
        ("4-4-2 (home) v 4-3-3", Formation::F442, Formation::F433),
        ("4-3-3 (home) v 4-4-2", Formation::F433, Formation::F442),
    ] {
        let mut by_value = Sum::default();
        let mut forced = Sum::default();
        for seed in 0..n {
            let (db, setup) = fm_match::demo::demo_match_with(seed, home, away);
            let mut e = MatchEngine::new(&setup, &db);
            let forced_at = e.state().tuning.value.forced_release_ticks;
            let min_hold = e.state().tuning.decision.min_hold_ticks;
            // Decisions of the spell in progress (outfield carriers with a
            // pass candidate only).
            let mut spell: Vec<OptionValues> = Vec::new();
            let mut holder = usize::MAX;
            let mut prev_ht = 0;
            while !e.is_finished() {
                e.tick_logic();
                let s = e.state();
                let frame = TickFrame::capture(s);
                match s.ball {
                    BallState::Held { holder: h } => {
                        let h = h as usize;
                        if h != holder || s.holder_ticks < prev_ht {
                            spell.clear();
                            holder = h;
                        }
                        let mut next = s.clone();
                        next.holder_ticks += 1;
                        if DecisionSystem::redecides(&next, &frame, h) {
                            let v = DecisionSystem::option_values(&next, &frame, h);
                            let waiting = next.holder_ticks < min_hold && !v.pressed;
                            if !waiting && v.pass_to.is_some() && v.dribble > f32::MIN {
                                spell.push(v);
                            }
                        }
                    }
                    BallState::Flight {
                        flight,
                        intent: FlightIntent::Pass { .. },
                    } if flight.kick_ms == s.now_ms() => {
                        let into = if prev_ht + 1 >= forced_at {
                            &mut forced
                        } else {
                            &mut by_value
                        };
                        for v in &spell {
                            into.add(v);
                        }
                        spell.clear();
                        holder = usize::MAX;
                    }
                    _ => {
                        spell.clear();
                        holder = usize::MAX;
                    }
                }
                prev_ht = s.holder_ticks;
            }
        }
        println!("== {name} ({n} matches)");
        by_value.print("spells ending in a pass by value", n);
        forced.print("spells ending at the forced release", n);
    }
}
