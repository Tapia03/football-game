//! `TuningParams`: every calibration constant of the match engine in one
//! place (spec Fase 5). `Default` is the current calibration; ranges are
//! `(min, max)` clamps. Attribute *weights* inside a formula (e.g. 0.6 ×
//! finishing + 0.2 × composure) describe the formula's shape and stay in code.

use crate::anchor::AnchorTuning;
use crate::events::RestartKind;

/// Carrier decision: hold / shoot / pass / dribble scoring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecisionTuning {
    /// An opponent this close means the carrier is pressed (m).
    pub pressure_radius: f32,
    /// Ticks a carrier keeps the ball before releasing it unless pressed.
    pub min_hold_ticks: u32,
    pub keeper_hold_ticks: u32,
    /// Shots are only considered inside this distance (m).
    pub shoot_range: f32,
    /// Minimum shot score to shoot.
    pub shoot_threshold: f32,
    /// Beyond this distance `long_shots` replaces `finishing` (m).
    pub long_shot_dist: f32,
    pub shoot_base: f32,
    pub shoot_skill: f32,
    pub shoot_gain: f32,
    /// Lateral distance at which centrality reaches 0 (m), and its floor.
    pub central_width: f32,
    pub central_min: f32,
    /// A defender within this distance of the shot line blocks it (m)…
    pub shot_block_dist: f32,
    /// …multiplying the shot score by this.
    pub shot_blocked_factor: f32,
    pub pass_min_dist: f32,
    pub pass_max_dist: f32,
    pub pass_base: f32,
    pub pass_progress_div: f32,
    /// Lane openness is capped at this (m).
    pub pass_lane_cap: f32,
    pub pass_open_div: f32,
    pub pass_len_div: f32,
    /// Lanes narrower than this are penalised (m)…
    pub pass_narrow_lane: f32,
    /// …by this much per metre missing.
    pub pass_narrow_penalty: f32,
    pub pass_to_keeper_penalty: f32,
    /// How far ahead the dribble space probe looks (m).
    pub dribble_probe_ahead: f32,
    pub dribble_space_min: f32,
    pub dribble_base: f32,
    pub dribble_space_cap: f32,
    pub dribble_space_div: f32,
    pub dribble_cramped: f32,
    /// Dribble value starts decaying after this many ticks on the ball…
    pub carry_decay_start: u32,
    /// …reaching zero this many ticks later.
    pub carry_decay_span: f32,
    pub near_goal_dist: f32,
    pub near_goal_factor: f32,
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
    pub save_base: f32,
    pub save_keeper: f32,
    pub save_skill: f32,
    pub save_dist_div: f32,
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
    /// A fresh receiver gets this many ticks before being challenged.
    pub settle_ticks: u32,
    /// Individual recovery after any challenge / after being beaten (ticks).
    pub cooldown_ticks: u32,
    pub beaten_cooldown_ticks: u32,
    /// Team-wide gap between challenges (ticks). Phase 4 stop-gap.
    pub team_gap_ticks: u32,
    pub win_base: f32,
    pub win_tackler: f32,
    pub win_carrier: f32,
    pub win_range: (f32, f32),
    /// Share of won tackles that come away clean (vs. ball knocked loose).
    pub clean_win: f32,
    /// Second defender covers this far goal-side of the carrier (m).
    pub cover_dist: f32,
    pub cover_urgency: f32,
}

/// Fouls and cards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisciplineTuning {
    pub foul_base: f32,
    pub foul_aggression: f32,
    pub foul_tackling: f32,
    pub foul_range: (f32, f32),
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

/// All engine calibration. `Default` = each block's current calibration.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TuningParams {
    pub anchor: AnchorTuning,
    pub decision: DecisionTuning,
    pub pass: PassTuning,
    pub shot: ShotTuning,
    pub duel: DuelTuning,
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
            // xG falls steeply with distance/angle; most shots < 18 m.
            shoot_range: 25.0,
            shoot_threshold: 0.60,
            long_shot_dist: 20.0,
            shoot_base: 0.25,
            shoot_skill: 0.75,
            shoot_gain: 2.2,
            central_width: 30.0,
            central_min: 0.2,
            shot_block_dist: 1.0,
            shot_blocked_factor: 0.3,
            pass_min_dist: 4.0,
            pass_max_dist: 45.0,
            pass_base: 0.4,
            pass_progress_div: 40.0,
            pass_lane_cap: 6.0,
            pass_open_div: 10.0,
            pass_len_div: 80.0,
            pass_narrow_lane: 2.5,
            pass_narrow_penalty: 0.25,
            pass_to_keeper_penalty: 0.3,
            dribble_probe_ahead: 6.0,
            dribble_space_min: 5.0,
            dribble_base: 0.55,
            dribble_space_cap: 20.0,
            dribble_space_div: 40.0,
            dribble_cramped: 0.15,
            // Long carries are rare: decays after ~2.5 s on the ball.
            carry_decay_start: 25,
            carry_decay_span: 30.0,
            near_goal_dist: 22.0,
            near_goal_factor: 0.5,
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
            // ~70% of on-target shots are saved in real football.
            save_base: 0.5,
            save_keeper: 0.4,
            save_skill: 0.3,
            save_dist_div: 80.0,
            save_range: (0.15, 0.92),
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
            settle_ticks: 3,
            cooldown_ticks: 25,
            beaten_cooldown_ticks: 40,
            team_gap_ticks: 70,
            // Tackling vs dribbling decides most duels; base ~35-45%.
            win_base: 0.35,
            win_tackler: 0.35,
            win_carrier: 0.3,
            win_range: (0.1, 0.75),
            clean_win: 0.6,
            cover_dist: 4.0,
            cover_urgency: 0.9,
        }
    }
}

impl Default for DisciplineTuning {
    fn default() -> Self {
        Self {
            foul_base: 0.03,
            foul_aggression: 0.08,
            foul_tackling: 0.03,
            foul_range: (0.02, 0.15),
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
            base: 0.6,
            touch: 0.37,
            easy_speed: 12.0,
            speed_penalty: 0.015,
            high_ball_penalty: 0.2,
            intercept_penalty: 0.25,
            range: (0.15, 0.97),
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
