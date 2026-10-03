//! `TuningParams`: every calibration constant of the match engine in one
//! place (spec Fase 5). `Default` is the current calibration; ranges are
//! `(min, max)` clamps. Attribute *weights* inside a formula (e.g. 0.6 ×
//! finishing + 0.2 × composure) describe the formula's shape and stay in code.

use crate::anchor::AnchorTuning;
use crate::events::RestartKind;
use crate::value::{KARUN_SINGH_XT, XT_COLS, XT_ROWS};

/// Carrier decision: hold / shoot / pass / dribble scoring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecisionTuning {
    /// An opponent this close means the carrier is pressed (m).
    pub pressure_radius: f32,
    /// Ticks a carrier keeps the ball before releasing it unless pressed.
    pub min_hold_ticks: u32,
    pub keeper_hold_ticks: u32,
    /// The carrier re-evaluates its options every this many ticks, and at
    /// once when it has just received the ball or is pressed.
    pub decision_cadence_ticks: u32,
    /// Shots are only considered inside this distance (m).
    pub shoot_range: f32,
    /// Beyond this distance `long_shots` replaces `finishing` (m).
    pub long_shot_dist: f32,
    pub pass_min_dist: f32,
    pub pass_max_dist: f32,
    /// How far ahead the dribble space probe looks (m).
    pub dribble_probe_ahead: f32,
    pub dribble_space_min: f32,
    /// Distance of each dribble step target (m).
    pub dribble_step: f32,
    /// Fraction of top speed when carrying / holding the ball.
    pub carry_urgency: f32,
    pub hold_urgency: f32,
}

/// Pass execution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PassTuning {
    /// Speed a ground pass still has at the receiver (m/s).
    pub arrival_speed: f32,
    /// Passes longer than this are lofted (m).
    pub lofted_dist: f32,
    /// Lofted hang time: `base + distance / per_m` (s).
    pub lofted_base_s: f32,
    pub lofted_per_m: f32,
    /// Max angular error: `base - skill·k + pressure·k` (rad).
    pub angle_base: f32,
    pub angle_skill: f32,
    pub angle_pressure: f32,
    /// Max relative length error: `base - skill·k`.
    pub length_base: f32,
    pub length_skill: f32,
    /// Clearance length (m, ±20%) and max angular spread (rad).
    pub clear_dist: f32,
    pub clear_angle: f32,
    /// Through ball: aimed this far beyond the runner's onside point (m),
    /// rolling into space at this speed when it gets there (m/s).
    pub through_lead: f32,
    pub through_arrival_speed: f32,
}

/// Shot execution and goalkeeping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotTuning {
    pub on_target_base: f32,
    pub on_target_skill: f32,
    pub on_target_dist_div: f32,
    pub on_target_pressure: f32,
    pub on_target_range: (f32, f32),
    pub penalty_on_base: f32,
    pub penalty_on_skill: f32,
    /// Goal probability of a shot = xG × finisher × keeper × pressure;
    /// keeper factor = `keeper_base - keeper_skill·gk`.
    pub keeper_base: f32,
    pub keeper_skill: f32,
    /// Fraction of the goal probability lost when the shooter is pressed.
    pub goal_pressure: f32,
    /// Save probability given on target, derived from the above, clamped.
    pub save_range: (f32, f32),
    pub penalty_save_base: f32,
    pub penalty_save_keeper: f32,
    pub penalty_save_skill: f32,
    pub penalty_save_range: (f32, f32),
    /// Inside this distance one-on-ones weigh more for the keeper (m).
    pub one_on_one_dist: f32,
    pub one_on_one_close: f32,
    pub one_on_one_far: f32,
    /// Shot speed: `base + skill·k` (m/s).
    pub speed_base: f32,
    pub speed_skill: f32,
}

/// Challenges for the ball.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DuelTuning {
    /// Reach of a tackle (m).
    pub tackle_range: f32,
    /// Individual recovery after any challenge / after being beaten (ticks).
    pub cooldown_ticks: u32,
    pub beaten_cooldown_ticks: u32,
    pub win_base: f32,
    pub win_tackler: f32,
    pub win_carrier: f32,
    pub win_range: (f32, f32),
    /// Change in win probability per unit of goal-side cos (front > behind).
    pub win_goal_side: f32,
    /// Share of won tackles that come away clean (vs. ball knocked loose).
    pub clean_win: f32,
    /// Second defender covers this far goal-side of the carrier (m).
    pub cover_dist: f32,
    pub cover_urgency: f32,
}

/// Defending the carrier: containment by danger zone and the challenge
/// decision (spec Fase 5, approved model).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DefendingTuning {
    /// Carrier within this distance of the defended goal: finishing zone (m).
    pub zone_box_dist: f32,
    /// Within this distance (and outside the box zone): box-entry zone (m).
    pub zone_mid_dist: f32,
    /// Containment distance `(tightest, loosest)` per zone (m). Inside a
    /// band, more aggressive defenders stand tighter.
    pub contain_box: (f32, f32),
    pub contain_mid: (f32, f32),
    pub contain_far: (f32, f32),
    pub contain_urgency: f32,
    /// Commit to a challenge when the score exceeds this.
    pub challenge_threshold: f32,
    pub challenge_base: f32,
    /// Weight of goal-side position: × cos(angle between defender and goal,
    /// seen from the carrier); +1 squarely goal-side, −1 chasing from behind.
    pub w_goal_side: f32,
    /// Bonus while the carrier has had the ball fewer than `fresh_ticks`.
    pub w_fresh: f32,
    pub fresh_ticks: u32,
    pub w_tackling: f32,
    pub w_decisions: f32,
    pub w_aggression: f32,
    /// Bonus in `TransitionDefense` (win it back now).
    pub transition_bonus: f32,
    /// Penalty when the carrier is inside the defender's own box.
    pub own_box_penalty: f32,
    /// A carrier who lingers becomes a target: bonus grows to `w_linger`
    /// over `linger_ticks` on the ball.
    pub w_linger: f32,
    pub linger_ticks: u32,
    /// Keepers may come out to smother a carrier inside their own box.
    pub keeper_smother: bool,
    /// A defender who decides to challenge from within this distance (m)
    /// closes on the carrier at full speed; the tackle resolves once in
    /// `DuelTuning::tackle_range`. With physics, containment holds 1–4 m
    /// off the carrier, so the challenge is a decision to close, not a
    /// coincidence of distance (spec Fase 5, invariant 18 on defence).
    pub engage_range: f32,
}

/// Fouls and cards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisciplineTuning {
    pub foul_base: f32,
    pub foul_aggression: f32,
    pub foul_tackling: f32,
    pub foul_range: (f32, f32),
    /// Extra foul probability when challenging from behind (× −cos).
    pub foul_from_behind: f32,
    /// Foul probability multiplier inside the defender's own box.
    pub own_box_factor: f32,
    /// Foul probability multiplier for an already-booked defender.
    pub booked_factor: f32,
    pub red_direct: f32,
    pub yellow_base: f32,
    pub yellow_aggression: f32,
}

/// Bringing a loose ball under control.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlTuning {
    pub control_radius: f32,
    pub receiver_radius: f32,
    pub intercept_radius: f32,
    pub keeper_reach: f32,
    /// Max ball height playable by outfielders / keepers in their box (m).
    pub max_height: f32,
    pub keeper_max_height: f32,
    /// Ticks a kicker must wait before touching the ball again.
    pub retouch_ticks: u32,
    pub base: f32,
    pub touch: f32,
    /// Ball speed above which control gets harder (m/s), and the slope.
    pub easy_speed: f32,
    pub speed_penalty: f32,
    pub high_ball_penalty: f32,
    pub intercept_penalty: f32,
    /// An opponent within `pressure_radius` of the receiver.
    pub pressure_penalty: f32,
    pub range: (f32, f32),
}

/// Time to set up each restart (ticks).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RestartTuning {
    pub kick_off: u32,
    pub throw_in: u32,
    pub goal_kick: u32,
    pub free_kick: u32,
    pub corner: u32,
    pub penalty: u32,
    /// Fraction of top speed at which the taker walks to the spot.
    pub taker_urgency: f32,
}

impl RestartTuning {
    #[must_use]
    pub const fn wait(&self, kind: RestartKind) -> u32 {
        match kind {
            RestartKind::KickOff => self.kick_off,
            RestartKind::ThrowIn => self.throw_in,
            RestartKind::GoalKick => self.goal_kick,
            RestartKind::FreeKick => self.free_kick,
            RestartKind::Corner => self.corner,
            RestartKind::Penalty => self.penalty,
        }
    }
}

/// Expected goals (spec Fase 5 (c1)): logistic in the goal-mouth angle and
/// the distance to goal, then scaled by the finisher.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct XgTuning {
    /// `logit(xG) = intercept + w_angle·angle(rad) + w_dist·distance(m)`.
    pub intercept: f32,
    pub w_angle: f32,
    pub w_dist: f32,
    /// Finisher factor: `skill_base + skill_k·skill` (1.0 at skill 0.5).
    pub skill_base: f32,
    pub skill_k: f32,
    /// Half-width of a body blocking the goal mouth (m).
    pub block_radius: f32,
}

impl Default for XgTuning {
    fn default() -> Self {
        // Fitted through central anchors in the range of public xG models:
        // 6 m ≈ 0.40, 11 m ≈ 0.17, 20 m ≈ 0.05, 25 m ≈ 0.03. Revisit in (c2).
        Self {
            intercept: -1.354,
            w_angle: 1.447,
            w_dist: -0.1057,
            skill_base: 0.6,
            skill_k: 0.8,
            block_radius: 0.5,
        }
    }
}

/// Common value currency of the carrier's options (spec Fase 5 (c1),
/// item 2): expected value = P(keep) × xT(after) − P(lose) × opponents'
/// xT(where lost); shots are worth their estimated xG.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueTuning {
    /// Expected Threat grid, `[row][col]` (see `value::KARUN_SINGH_XT`).
    pub xt: [[f32; XT_COLS]; XT_ROWS],
    /// An action must beat keeping the ball by this much to be taken.
    pub act_margin: f32,
    /// Pass interception estimate: an opponent cuts a pass if, by the time
    /// the ball passes him, his reach (`body_reach` + top speed × time
    /// after `react_s`) covers his distance to the lane. The chance falls
    /// linearly from `intercept_max` (on the lane) to 0 (at the reach).
    /// The ball is assumed to travel at `pass_speed` m/s on average.
    pub pass_intercept_max: f32,
    pub pass_speed: f32,
    pub react_s: f32,
    /// Extra time an outfield defender needs to turn when the ball is
    /// played behind them (through-ball race, s).
    pub turn_s: f32,
    pub body_reach: f32,
    /// A lofted pass can only be cut within this distance of the kick, or
    /// of where it lands (m).
    pub lofted_takeoff: f32,
    pub lofted_landing: f32,
    /// Pass accuracy: `1 − d · error_per_m · (1.5 − skill)`, floored.
    pub pass_error_per_m: f32,
    pub pass_accuracy_min: f32,
    /// Keeping the ball while dribbling into space, and into a marker:
    /// `cramped_base + cramped_skill · dribbling`.
    pub dribble_keep_open: f32,
    pub dribble_keep_cramped_base: f32,
    pub dribble_keep_cramped_skill: f32,
    /// Item 5 of (c1) is paused (spec): through balls and the lofted-pass
    /// lane model are implemented but off until the user decides.
    pub through_balls: bool,
    pub lofted_lane: bool,
    /// Through ball race: the runner wins if it gets there first; the
    /// chance moves 0 → 1 over this many seconds of margin (centred on 0).
    pub race_scale_s: f32,
    /// First touch on a ball met on the run is this much harder.
    pub through_control_penalty: f32,
    /// Keeping the ball when holding it under pressure.
    pub hold_keep_pressed: f32,
    /// Share of the current threat a static possession loses per tick on
    /// the ball (the defence settles while the carrier waits): holding is
    /// worth `1 − erosion · ticks_on_ball` of the threat, floored at 0.
    pub hold_erosion: f32,
    /// After this many ticks on the ball the carrier must release it (pass
    /// or clear): the deadlock guard of Fase 5 (b).
    pub forced_release_ticks: u32,
}

impl Default for ValueTuning {
    fn default() -> Self {
        Self {
            xt: KARUN_SINGH_XT,
            // ~0.2% goal probability: below that, passing is not worth it.
            act_margin: 0.002,
            // A defender squarely in the lane cuts most passes.
            pass_intercept_max: 0.7,
            // ~14 m/s average over a ground pass; 0.25 s to react; a leg
            // reaches ~0.8 m.
            pass_speed: 14.0,
            react_s: 0.25,
            turn_s: 0.5,
            body_reach: 0.8,
            lofted_takeoff: 2.0,
            lofted_landing: 5.0,
            // Average passer: ~88% on target at 30 m; elite ~94%.
            pass_error_per_m: 0.004,
            pass_accuracy_min: 0.5,
            dribble_keep_open: 0.97,
            // An average dribbler keeps it ~65% of the time into a marker.
            dribble_keep_cramped_base: 0.45,
            dribble_keep_cramped_skill: 0.4,
            through_balls: false,
            lofted_lane: true,
            race_scale_s: 1.5,
            through_control_penalty: 0.05,
            hold_keep_pressed: 0.85,
            // Worth nothing after ~3.3 s of waiting.
            hold_erosion: 0.03,
            // 5.5 s on the ball (the old carry-decay end).
            forced_release_ticks: 55,
        }
    }
}

/// Player movement physics (spec Fase 5, "Física de movimento").
/// `max_accel = ∞`, `reaction_ms = 0`, `turn_rate = ∞` reproduce the
/// original instant-velocity model exactly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KinematicsTuning {
    /// Largest change of velocity per second (m/s²).
    pub max_accel: f32,
    /// Delay before reacting to a new intent (ms)…
    pub reaction_ms: u32,
    /// …when the target moves by more than this (m).
    pub reaction_threshold: f32,
    /// Largest heading change while moving (rad/s).
    pub turn_rate: f32,
}

impl Default for KinematicsTuning {
    fn default() -> Self {
        // Conservative values that keep the engine running (spec Fase 5,
        // "Orçamento pontual"); realistic ones (4.5 m/s², 200 ms, 6 rad/s)
        // once the engine is stable.
        Self {
            max_accel: 15.0,
            reaction_ms: 80,
            reaction_threshold: 2.0,
            turn_rate: 15.0,
        }
    }
}

/// Off-ball runs of strikers and wingers (spec Fase 5 (c1), item 4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunTuning {
    /// Length of a run (ticks).
    pub run_ticks: u32,
    /// Recovery before the same player runs again: `base − skill ·
    /// off_the_ball` (ticks). How often a side runs is decided in
    /// `plan_runs`, not by this.
    pub cooldown_base: f32,
    pub cooldown_skill: f32,
    /// How far short of the offside line the runner aims (m).
    pub onside_margin: f32,
    /// Minimum room between runner and line to bother running (m).
    pub min_room: f32,
    /// Lateral search for the open lane: ± this much (m), 5 lanes.
    pub lane_reach: f32,
    /// Defenders within this depth of the line count for the lane (m).
    pub line_depth: f32,
    /// Lanes stay this far inside the touchlines (m).
    pub touchline_margin: f32,
    /// Fraction of top speed while running.
    pub urgency: f32,
    /// Runs a side may have live at once, whatever its formation.
    pub max_runners: u32,
    /// The run's marker stands this far goal-side of the runner (m), moving
    /// at this fraction of top speed.
    pub mark_dist: f32,
    pub mark_urgency: f32,
}

impl Default for RunTuning {
    fn default() -> Self {
        Self {
            // A run in behind lasts ~2.5 s.
            run_ticks: 25,
            // Every ~6 s for a poor mover, every ~2 s for an elite one.
            cooldown_base: 60.0,
            cooldown_skill: 40.0,
            onside_margin: 0.5,
            min_room: 4.0,
            lane_reach: 8.0,
            line_depth: 10.0,
            touchline_margin: 3.0,
            urgency: 0.95,
            // One run at a time: the others stay as short support.
            max_runners: 1,
            // Touch-tight but goal-side; tracking a run is a sprint.
            mark_dist: 1.5,
            mark_urgency: 0.95,
        }
    }
}

/// All engine calibration. `Default` = each block's current calibration.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TuningParams {
    pub anchor: AnchorTuning,
    pub decision: DecisionTuning,
    pub pass: PassTuning,
    pub shot: ShotTuning,
    pub xg: XgTuning,
    pub value: ValueTuning,
    pub runs: RunTuning,
    pub kinematics: KinematicsTuning,
    pub duel: DuelTuning,
    pub defending: DefendingTuning,
    pub discipline: DisciplineTuning,
    pub control: ControlTuning,
    pub restart: RestartTuning,
}

impl Default for DecisionTuning {
    fn default() -> Self {
        Self {
            pressure_radius: 2.5,
            // First touch + look up ≈ 0.4 s; keepers ≈ 1.5 s.
            min_hold_ticks: 4,
            keeper_hold_ticks: 15,
            // ~0.3 s between looks: nobody re-reads ten passing options
            // every 100 ms.
            decision_cadence_ticks: 3,
            // Beyond ~30 m the xG of any shot is < 0.02: not worth computing.
            shoot_range: 30.0,
            long_shot_dist: 20.0,
            pass_min_dist: 4.0,
            pass_max_dist: 45.0,
            dribble_probe_ahead: 6.0,
            dribble_space_min: 5.0,
            dribble_step: 5.0,
            // Running with the ball ≈ 60% of sprint speed.
            carry_urgency: 0.6,
            hold_urgency: 0.3,
        }
    }
}

impl Default for PassTuning {
    fn default() -> Self {
        Self {
            // ~8 m/s at the receiver: a 20 m pass takes ~1.8 s.
            arrival_speed: 8.0,
            lofted_dist: 28.0,
            lofted_base_s: 0.9,
            lofted_per_m: 30.0,
            // Elite passers miss by ~2-3°, poor ones by ~12°.
            angle_base: 0.22,
            angle_skill: 0.18,
            angle_pressure: 0.08,
            length_base: 0.15,
            length_skill: 0.12,
            // A hoofed clearance: ~45 m, up to ~0.5 rad off straight.
            clear_dist: 45.0,
            clear_angle: 0.5,
            // Into the space behind the line, dying for the runner.
            through_lead: 8.0,
            through_arrival_speed: 0.5,
        }
    }
}

impl Default for ShotTuning {
    fn default() -> Self {
        Self {
            on_target_base: 0.25,
            on_target_skill: 0.5,
            on_target_dist_div: 60.0,
            on_target_pressure: 0.1,
            on_target_range: (0.05, 0.85),
            penalty_on_base: 0.8,
            penalty_on_skill: 0.12,
            // An average keeper (gk 0.5) leaves the xG unchanged; the best
            // concede ~20% fewer, the worst ~20% more.
            keeper_base: 1.2,
            keeper_skill: 0.4,
            goal_pressure: 0.15,
            save_range: (0.05, 0.97),
            penalty_save_base: 0.12,
            penalty_save_keeper: 0.2,
            penalty_save_skill: 0.1,
            penalty_save_range: (0.05, 0.35),
            one_on_one_dist: 12.0,
            one_on_one_close: 0.25,
            one_on_one_far: 0.1,
            speed_base: 20.0,
            speed_skill: 8.0,
        }
    }
}

impl Default for DuelTuning {
    fn default() -> Self {
        Self {
            tackle_range: 1.8,
            cooldown_ticks: 25,
            beaten_cooldown_ticks: 40,
            // Tackling vs dribbling decides most duels; base ~35-45%.
            win_base: 0.35,
            win_tackler: 0.35,
            win_carrier: 0.3,
            win_range: (0.1, 0.75),
            // Goal-side challenges win more often than ones from behind.
            win_goal_side: 0.1,
            clean_win: 0.6,
            cover_dist: 4.0,
            cover_urgency: 0.9,
        }
    }
}

impl Default for DefendingTuning {
    fn default() -> Self {
        Self {
            // Penalty-area depth ≈ 16.5 m; "box entry" ≈ up to 35 m out.
            zone_box_dist: 18.0,
            zone_mid_dist: 35.0,
            contain_box: (1.0, 2.0),
            contain_mid: (2.0, 3.0),
            contain_far: (3.0, 4.0),
            contain_urgency: 1.0,
            challenge_threshold: 1.15,
            challenge_base: 0.0,
            w_goal_side: 0.4,
            w_fresh: 0.25,
            fresh_ticks: 6,
            w_tackling: 0.3,
            w_decisions: 0.15,
            w_aggression: 0.25,
            transition_bonus: 0.15,
            own_box_penalty: 0.35,
            w_linger: 0.0,
            linger_ticks: 30,
            keeper_smother: true,
            engage_range: 4.5,
        }
    }
}

impl Default for DisciplineTuning {
    fn default() -> Self {
        Self {
            foul_base: 0.26,
            foul_aggression: 0.24,
            foul_tackling: 0.03,
            foul_range: (0.05, 0.45),
            foul_from_behind: 0.2,
            own_box_factor: 0.05,
            booked_factor: 0.3,
            // ~1 card per 6-7 fouls; direct reds ~1 per 250 fouls.
            red_direct: 0.004,
            yellow_base: 0.07,
            yellow_aggression: 0.13,
        }
    }
}

impl Default for ControlTuning {
    fn default() -> Self {
        Self {
            control_radius: 1.2,
            receiver_radius: 1.5,
            intercept_radius: 0.9,
            keeper_reach: 2.2,
            max_height: 1.8,
            keeper_max_height: 2.6,
            retouch_ticks: 5,
            // Professional first touch rarely fails on a normal pass: a
            // typical receiver (touch 0.5) controls ~90%, a good one ~95%;
            // fast, high or contested balls are harder.
            base: 0.82,
            touch: 0.16,
            easy_speed: 12.0,
            speed_penalty: 0.015,
            high_ball_penalty: 0.15,
            intercept_penalty: 0.25,
            pressure_penalty: 0.06,
            range: (0.15, 0.98),
        }
    }
}

impl Default for RestartTuning {
    fn default() -> Self {
        Self {
            kick_off: 30,
            throw_in: 15,
            goal_kick: 25,
            free_kick: 25,
            corner: 40,
            penalty: 40,
            taker_urgency: 0.9,
        }
    }
}
