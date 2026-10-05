//! `MatchEngine`: the single logical tick (spec 3.A). One call to
//! `tick_logic` advances the match by exactly `LOGICAL_DT_MS`, identically for
//! every LOD. It performs no heap allocation after construction (criterion 17).

use fm_core::{pitch, GoalEnd, Vec2};
use fm_entities::{PlayerDatabase, PlayerId};

use crate::anchor::FormationAnchor;
use crate::decision::{Action, DecisionSystem};
use crate::events::{EventKind, EventLog, MatchEvent, RestartKind};
use crate::formation::{Formation, Role};
use crate::kinematics::{PlayerKinematics, TurnLimit};
use crate::phase::{Phase, PhaseStateMachine, Side, LOGICAL_DT_MS};
use crate::resolver::ActionResolver;
use crate::role::{RoleBehavior, RoleContext, RoleIntent};
use crate::snapshot::{LodLevel, MatchSnapshot, PlayerSnapshot};
use crate::state::{
    side_index, BallState, FlightIntent, MatchPlayer, MatchState, Restart, ShotOutcome, TeamState,
    PLAYERS,
};
use crate::tactics::Tactics;
use crate::tick_frame::TickFrame;
use crate::tuning::TuningParams;

/// 45 minutes of logical ticks.
pub const HALF_TICKS: u32 = 45 * 60 * 1_000 / LOGICAL_DT_MS;
/// 90 minutes (no stoppage time in the MVP).
pub const FULL_TICKS: u32 = 2 * HALF_TICKS;

/// A team's line-up: `players[i]` plays slot `i` of `formation`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TeamSheet {
    pub formation: Formation,
    pub tactics: Tactics,
    pub players: [PlayerId; 11],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchSetup {
    /// Stored in the save to replay the match (spec 3.B).
    pub match_seed: u64,
    pub home: TeamSheet,
    pub away: TeamSheet,
    pub tuning: TuningParams,
}

pub struct MatchEngine {
    state: MatchState,
}

/// Puts the ball dead at `spot` for `side` to restart with `kind`.
///
/// # Panics
/// Never in practice: player indices are always `< 22`.
pub fn set_restart(
    state: &mut MatchState,
    frame: &TickFrame,
    kind: RestartKind,
    side: Side,
    spot: Vec2,
) {
    let want_keeper = kind == RestartKind::GoalKick;
    let mut taker = None;
    let mut best = f32::MAX;
    for (i, p) in state.players.iter().enumerate() {
        if p.side != side || !p.active() {
            continue;
        }
        let is_keeper = p.role == Role::Goalkeeper;
        if want_keeper != is_keeper && !(want_keeper && state.keeper(side).is_none()) {
            continue;
        }
        let d = frame.pos(i).distance(spot);
        if d < best {
            best = d;
            taker = Some(i);
        }
    }
    let taker = taker.or_else(|| {
        state
            .players
            .iter()
            .position(|p| p.side == side && p.active())
    });
    let Some(taker) = taker else {
        return;
    };
    // The ball is put back in play from inside the lines: the taker takes
    // it within 1 m of the spot and carries it 0.5 m ahead of himself, and
    // a carried ball beyond a line is out of play.
    let m = state.tuning.restart.edge_margin;
    let spot = spot.clamp(
        Vec2::new(m, m),
        Vec2::new(pitch::LENGTH - m, pitch::WIDTH - m),
    );
    state.ball = BallState::Dead(Restart {
        kind,
        side,
        spot,
        taker: u8::try_from(taker).expect("< 22"),
        ready_tick: state.tick + state.tuning.restart.wait(kind),
    });
    state.holder_ticks = 0;
    state.events.push(MatchEvent {
        tick: state.tick,
        kind: EventKind::Restart { side, kind },
    });
}

impl MatchEngine {
    /// Builds the match, copying the 22 starters' attributes out of `db`.
    ///
    /// # Panics
    /// If a team sheet references a player not in `db`.
    #[must_use]
    pub fn new(setup: &MatchSetup, db: &PlayerDatabase) -> Self {
        let placeholder = MatchPlayer {
            id: PlayerId(0),
            side: Side::Home,
            slot: 0,
            role: Role::Goalkeeper,
            attrs: db.static_of(setup.home.players[0]).attributes,
            top_speed: 0.0,
            traj: PlayerKinematics::plan_trajectory(Vec2::ZERO, Vec2::ZERO, 0.0, 0),
            lead: None,
            action_count: 0,
            yellow_cards: 0,
            sent_off: false,
            tackle_ready_tick: 0,
            run_target: Vec2::ZERO,
            run_until: 0,
            run_ready_tick: 0,
            touch_ready_tick: 0,
        };
        let mut players = [placeholder; PLAYERS];
        for (side, sheet) in [(Side::Home, &setup.home), (Side::Away, &setup.away)] {
            for (slot, id) in sheet.players.iter().enumerate() {
                let st = db.static_of(*id);
                let i = side_index(side) * 11 + slot;
                players[i] = MatchPlayer {
                    id: *id,
                    side,
                    slot: u8::try_from(slot).expect("11 slots"),
                    role: sheet.formation.slots()[slot].role,
                    attrs: st.attributes,
                    top_speed: PlayerKinematics::top_speed(st.attributes.physical.pace),
                    ..placeholder
                };
            }
        }
        let team = |sheet: &TeamSheet| TeamState {
            formation: sheet.formation,
            tactics: sheet.tactics,
            score: 0,
            shots: 0,
            shots_on_target: 0,
            passes: 0,
            passes_completed: 0,
            tackles: 0,
            #[cfg(feature = "diagnostics")]
            decisions: 0,
            #[cfg(feature = "diagnostics")]
            through_passes: 0,
            #[cfg(feature = "diagnostics")]
            through_completed: 0,
            #[cfg(feature = "diagnostics")]
            xg: 0.0,
        };
        let mut state = MatchState {
            match_seed: setup.match_seed,
            tick: 0,
            players,
            teams: [team(&setup.home), team(&setup.away)],
            ball: BallState::Dead(Restart {
                kind: RestartKind::KickOff,
                side: Side::Home,
                spot: pitch::CENTRE,
                taker: 0,
                ready_tick: 0,
            }),
            last_touch: Side::Home,
            holder_ticks: 0,
            carrier_plan: Action::Hold,
            phase_sm: PhaseStateMachine::new(Side::Home),
            phases: [Phase::SetPiece; 2],
            second_half: false,
            finished: false,
            tuning: setup.tuning,
            events: EventLog::new(),
            #[cfg(feature = "diagnostics")]
            through_kick_ms: None,
        };
        line_up_for_kickoff(&mut state, Side::Home);
        Self { state }
    }

    #[must_use]
    pub fn state(&self) -> &MatchState {
        &self.state
    }

    /// Changes the instructions of `side` during the match (spec Fase 6,
    /// 6C): in force from the next `tick_logic`. A match is a function of
    /// its seed and of these calls with the tick each one was made at.
    pub fn set_tactics(&mut self, side: Side, tactics: Tactics) {
        self.state.team_mut(side).tactics = tactics;
    }

    #[must_use]
    pub fn events(&self) -> &[MatchEvent] {
        self.state.events.as_slice()
    }

    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.state.finished
    }

    /// Advances the match by one logical tick (100 ms).
    pub fn tick_logic(&mut self) {
        let s = &mut self.state;
        if s.finished {
            return;
        }
        s.tick += 1;
        if s.tick == HALF_TICKS {
            s.events.push(MatchEvent {
                tick: s.tick,
                kind: EventKind::HalfTime,
            });
            s.second_half = true;
            line_up_for_kickoff(s, Side::Away);
            return;
        }
        if s.tick >= FULL_TICKS {
            s.events.push(MatchEvent {
                tick: s.tick,
                kind: EventKind::FullTime,
            });
            s.finished = true;
            return;
        }

        // Stage 1: positions of the 22 at this tick (constant until
        // `move_players` re-plans at the end).
        let mut frame = TickFrame::capture(s);
        step_ball(s, &frame);

        // Stage 2: possession → phases → anchors, for the ball as stepped.
        frame.observe_ball(s);
        let (home, away) = s.phase_sm.update(&frame);
        s.phases = [home, away];
        frame.set_phases(s.phases);
        frame.compute_anchors(s);
        crate::runs::plan_runs(s, &mut frame);

        let mut targets = [Vec2::ZERO; PLAYERS];
        let mut urgency = [0.6_f32; PLAYERS];
        // Who is in the play this tick (spec Fase 5, "Física só para quem
        // está no lance"): marked where a play-specific target is given.
        let mut in_play = [false; PLAYERS];
        plan_shape(s, &frame, &mut targets, &mut urgency, &mut in_play);
        on_ball(s, &frame, &mut targets, &mut urgency, &mut in_play);
        move_players(s, &frame, &targets, &urgency, &in_play);
    }

    /// Snapshot at `t_ms` (between this tick and the next). `None` in
    /// `Abstract`. Read-only by construction (`&self`).
    #[must_use]
    pub fn sample(&self, lod: LodLevel, t_ms: u32) -> Option<MatchSnapshot> {
        if lod == LodLevel::Abstract {
            return None;
        }
        let s = &self.state;
        let mut players = [PlayerSnapshot {
            id: 0,
            pos: Vec2::ZERO,
            side: Side::Home,
            sent_off: false,
        }; PLAYERS];
        for (out, p) in players.iter_mut().zip(&s.players) {
            *out = PlayerSnapshot {
                id: p.id.0,
                pos: p.pos(t_ms),
                side: p.side,
                sent_off: p.sent_off,
            };
        }
        Some(MatchSnapshot {
            t_ms,
            tick: s.tick,
            players,
            ball: s.ball_pos_at(t_ms),
            score: [s.teams[0].score, s.teams[1].score],
            phases: s.phases,
        })
    }

    /// Plays the whole match, handing every snapshot the LOD calls for to
    /// `on_snapshot`.
    pub fn run(&mut self, lod: LodLevel, mut on_snapshot: impl FnMut(&MatchSnapshot)) {
        while !self.is_finished() {
            self.tick_logic();
            let now = self.state.now_ms();
            for off in lod.sample_offsets_ms() {
                if let Some(snap) = self.sample(lod, now + off) {
                    on_snapshot(&snap);
                }
            }
        }
    }
}

/// Teleports everyone to their set-piece anchors (start and half time only).
fn line_up_for_kickoff(s: &mut MatchState, kicking: Side) {
    let now = s.now_ms();
    for i in 0..PLAYERS {
        let p = s.players[i];
        if !p.active() {
            continue;
        }
        let team = s.team(p.side);
        let slot = &team.formation.slots()[p.slot as usize];
        let at = FormationAnchor::compute(
            slot,
            pitch::CENTRE,
            Phase::SetPiece,
            team.tactics,
            s.frame(p.side),
            &s.tuning.anchor,
        );
        s.players[i].traj = PlayerKinematics::plan_trajectory(at, at, 1.0, now);
        s.players[i].lead = None;
    }
    s.phase_sm = PhaseStateMachine::new(kicking);
    let frame = &TickFrame::capture(s);
    set_restart(s, frame, RestartKind::KickOff, kicking, pitch::CENTRE);
    if let BallState::Dead(r) = &mut s.ball {
        r.ready_tick = s.tick;
    }
}

/// Ball physics consequences: shots arriving, ball out of play, loose-ball
/// control, restarts being taken.
fn step_ball(s: &mut MatchState, frame: &TickFrame) {
    let now = s.now_ms();
    match s.ball {
        BallState::Flight { intent, .. } => {
            if let FlightIntent::Shot {
                shooter,
                outcome,
                arrive_ms,
            } = intent
            {
                if now >= arrive_ms {
                    apply_shot(s, frame, shooter as usize, outcome);
                    return;
                }
            }
            let pos = frame.ball(s).xy();
            if !pitch::in_pitch(pos) {
                out_of_play(s, frame, pos);
                return;
            }
            let mut cands = [0_u8; PLAYERS];
            let n = ActionResolver::receive_candidates(s, frame, &mut cands);
            if n > 0 {
                // Only the nearest player gets a touch this tick; if they
                // miscontrol, the ball has a new path and others try next tick.
                ActionResolver::try_receive(s, frame, cands[0] as usize);
            }
        }
        BallState::Held { holder } => {
            let pos = frame.ball(s).xy();
            if !pitch::in_pitch(pos) {
                s.last_touch = s.players[holder as usize].side;
                out_of_play(s, frame, pos);
            }
        }
        BallState::Dead(r) => {
            let mut taker = r.taker as usize;
            if !s.players[taker].active() {
                // Taker sent off meanwhile: re-elect.
                set_restart(s, frame, r.kind, r.side, r.spot);
                let BallState::Dead(nr) = s.ball else { return };
                taker = nr.taker as usize;
            }
            let near = frame.pos(taker).distance(r.spot) < 1.0;
            if s.tick >= r.ready_tick && near {
                s.ball = BallState::Held {
                    holder: u8::try_from(taker).expect("< 22"),
                };
                s.holder_ticks = 0;
                s.last_touch = r.side;
                if r.kind == RestartKind::Penalty {
                    ActionResolver::take_penalty(s, frame, taker);
                }
            }
        }
    }
}

fn apply_shot(s: &mut MatchState, frame: &TickFrame, shooter: usize, outcome: ShotOutcome) {
    let side = s.players[shooter].side;
    let defending = side.other();
    match outcome {
        ShotOutcome::Goal => {
            s.team_mut(side).score += 1;
            s.events.push(MatchEvent {
                tick: s.tick,
                kind: EventKind::Goal {
                    side,
                    scorer: s.players[shooter].id,
                },
            });
            set_restart(s, frame, RestartKind::KickOff, defending, pitch::CENTRE);
        }
        ShotOutcome::Saved => {
            if let Some(k) = s.keeper(defending) {
                s.events.push(MatchEvent {
                    tick: s.tick,
                    kind: EventKind::Save {
                        keeper: s.players[k].id,
                    },
                });
                s.ball = BallState::Held {
                    holder: u8::try_from(k).expect("< 22"),
                };
                s.holder_ticks = 0;
                s.last_touch = defending;
            } else {
                goal_kick(s, frame, defending);
            }
        }
        ShotOutcome::OffTarget => goal_kick(s, frame, defending),
    }
}

fn goal_kick(s: &mut MatchState, frame: &TickFrame, side: Side) {
    let own = s.attacking(side).opposite();
    let x = own.goal_line_x() - own.direction() * pitch::GOAL_AREA_DEPTH;
    set_restart(
        s,
        frame,
        RestartKind::GoalKick,
        side,
        Vec2::new(x, pitch::HALF_WIDTH),
    );
}

/// The ball left the pitch at `pos`.
fn out_of_play(s: &mut MatchState, frame: &TickFrame, pos: Vec2) {
    let to = s.last_touch.other();
    if pos.y < 0.0 || pos.y > pitch::WIDTH {
        let spot = Vec2::new(
            pos.x.clamp(0.0, pitch::LENGTH),
            pos.y.clamp(0.0, pitch::WIDTH),
        );
        set_restart(s, frame, RestartKind::ThrowIn, to, spot);
        return;
    }
    let end = if pos.x < 0.0 {
        GoalEnd::Left
    } else {
        GoalEnd::Right
    };
    // The side defending `end` is the one attacking the other way.
    let defending = if s.attacking(Side::Home) == end {
        Side::Away
    } else {
        Side::Home
    };
    if s.last_touch == defending {
        let y = if pos.y < pitch::HALF_WIDTH {
            0.0
        } else {
            pitch::WIDTH
        };
        set_restart(
            s,
            frame,
            RestartKind::Corner,
            defending.other(),
            Vec2::new(end.goal_line_x(), y),
        );
    } else {
        goal_kick(s, frame, defending);
    }
}

/// Every player's shape target from anchors and role behaviour.
fn plan_shape(
    s: &MatchState,
    frame: &TickFrame,
    targets: &mut [Vec2; PLAYERS],
    urgency: &mut [f32; PLAYERS],
    in_play: &mut [bool; PLAYERS],
) {
    let ball = frame.ball(s).xy();
    for (i, p) in s.players.iter().enumerate() {
        if !p.active() {
            continue;
        }
        let RoleIntent::MoveTo { target, urgency: u } = RoleBehavior::update(
            p.role,
            &RoleContext {
                anchor: frame.anchor(i),
                ball,
                phase: frame.phase(p.side),
            },
        );
        targets[i] = target;
        urgency[i] = u;
        // A live off-ball run overrides the shape (spec Fase 5 (c1), item 4).
        if p.run_until > s.tick {
            if let Some((run_target, run_urgency)) = crate::runs::active_run(s, frame, i) {
                targets[i] = run_target;
                in_play[i] = true;
                urgency[i] = run_urgency;
            }
        }
    }
}

/// Ball-related behaviour: the carrier decides, the nearest defender presses
/// and tackles, chasers go for loose balls, takers walk to restarts.
fn on_ball(
    s: &mut MatchState,
    frame: &TickFrame,
    targets: &mut [Vec2; PLAYERS],
    urgency: &mut [f32; PLAYERS],
    in_play: &mut [bool; PLAYERS],
) {
    let now = s.now_ms();
    match s.ball {
        BallState::Held { holder } => {
            let h = holder as usize;
            s.holder_ticks += 1;
            let holder_side = s.players[h].side;
            // Decision cadence: between decisions the carrier keeps doing
            // what it last chose (hold or dribble on to the same target).
            let action = if DecisionSystem::redecides(s, frame, h) {
                #[cfg(feature = "diagnostics")]
                {
                    s.team_mut(holder_side).decisions += 1;
                }
                DecisionSystem::choose_action(s, frame, h)
            } else {
                s.carrier_plan
            };
            s.carrier_plan = action;
            ActionResolver::resolve(s, frame, h, action);
            let here = frame.pos(h);
            match action {
                Action::Dribble { target } => {
                    targets[h] = target;
                    in_play[h] = true;
                    urgency[h] = s.tuning.decision.carry_urgency;
                }
                Action::Hold => {
                    targets[h] = here;
                    in_play[h] = true;
                    urgency[h] = s.tuning.decision.hold_urgency;
                }
                Action::Pass { .. }
                | Action::ThroughPass { .. }
                | Action::Shoot
                | Action::Clear
                | Action::Tackle { .. } => {}
            }
            // Defending the carrier (if they still have it): the nearest
            // defender contains goal-side at the zone's distance, the second
            // covers behind them, and whoever is within engage range decides
            // whether to commit to a challenge (spec Fase 5 defending model).
            if let BallState::Held { holder } = s.ball {
                let carrier = holder as usize;
                let defending = holder_side.other();
                let hpos = frame.pos(carrier);
                let own_goal = s.attacking(defending).opposite().goal_centre();
                let (first, second) = two_nearest(s, frame, defending, hpos);
                if let Some(d) = first {
                    targets[d] = DecisionSystem::containment_point(s, frame, d, carrier);
                    in_play[d] = true;
                    urgency[d] = s.tuning.defending.contain_urgency;
                }
                if let Some(d) = second {
                    let behind = s.tuning.defending.contain_far.1 + s.tuning.duel.cover_dist;
                    targets[d] = hpos + (own_goal - hpos).normalize() * behind;
                    in_play[d] = true;
                    urgency[d] = s.tuning.duel.cover_urgency;
                }
                // The challenger closes on the carrier; the tackle happens
                // only once they actually arrive (physics decides when).
                if let Some(d) = DecisionSystem::choose_challenger(s, frame, defending, carrier) {
                    if frame.pos(d).distance(hpos) < s.tuning.duel.tackle_range {
                        ActionResolver::resolve(s, frame, d, Action::Tackle { on: holder });
                    } else {
                        targets[d] = hpos;
                        in_play[d] = true;
                        urgency[d] = 1.0;
                    }
                }
            }
        }
        BallState::Flight { flight, intent } => {
            // Each side sends the player who can reach the ball first, to
            // the point where they can meet it (not where it is now).
            let ball = frame.ball(s).xy();
            let mut path = FlightPath::new(&flight, now);
            for side in [Side::Home, Side::Away] {
                if let Some(c) = nearest_to(s, frame, side, ball) {
                    let p = &s.players[c];
                    targets[c] = intercept_point(&mut path, frame.pos(c), p.top_speed);
                    in_play[c] = true;
                    urgency[c] = 1.0;
                }
            }
            if let FlightIntent::Pass { receiver } = intent {
                let r = receiver as usize;
                if s.players[r].active() {
                    let p = &s.players[r];
                    targets[r] = intercept_point(&mut path, frame.pos(r), p.top_speed);
                    in_play[r] = true;
                    urgency[r] = 1.0;
                }
            }
        }
        BallState::Dead(r) => {
            targets[r.taker as usize] = r.spot;
            in_play[r.taker as usize] = true;
            urgency[r.taker as usize] = s.tuning.restart.taker_urgency;
        }
    }
}

/// Ball positions along a flight at `now + step * 100 ms` (step 1..=30),
/// evaluated lazily and shared by every player chasing the same flight this
/// tick. Values are identical to calling `pos_at` directly.
struct FlightPath<'a> {
    flight: &'a crate::ball::BallFlight,
    now: u32,
    pts: [Vec2; INTERCEPT_STEPS as usize],
    filled: u32,
}

/// Look-ahead of `intercept_point`: 30 × 100 ms = 3 s.
const INTERCEPT_STEPS: u32 = 30;

impl<'a> FlightPath<'a> {
    fn new(flight: &'a crate::ball::BallFlight, now: u32) -> Self {
        Self {
            flight,
            now,
            pts: [Vec2::ZERO; INTERCEPT_STEPS as usize],
            filled: 0,
        }
    }

    fn at(&mut self, step: u32) -> Vec2 {
        while self.filled < step {
            self.filled += 1;
            let t = self.now + self.filled * LOGICAL_DT_MS;
            self.pts[(self.filled - 1) as usize] = self.flight.pos_at(t).xy();
        }
        self.pts[(step - 1) as usize]
    }
}

/// Earliest point on the ball's path a player at `from` running at `speed`
/// can reach in time (sampled every 100 ms over 3 s; no allocation). Falls
/// back to where the ball ends up after the look-ahead.
fn intercept_point(path: &mut FlightPath<'_>, from: Vec2, speed: f32) -> Vec2 {
    for step in 1..=INTERCEPT_STEPS {
        let b = path.at(step);
        #[allow(clippy::cast_precision_loss)] // ≤ 3000
        let reach = speed * (step * LOGICAL_DT_MS) as f32 / 1000.0;
        if from.distance(b) <= reach {
            return b;
        }
    }
    path.at(INTERCEPT_STEPS)
}

/// The two outfield players of `side` nearest to `at` (index breaks ties).
fn two_nearest(
    s: &MatchState,
    frame: &TickFrame,
    side: Side,
    at: Vec2,
) -> (Option<usize>, Option<usize>) {
    let (mut a, mut b) = (None, None);
    let (mut da, mut db) = (f32::MAX, f32::MAX);
    for (i, p) in s.players.iter().enumerate() {
        if p.side != side || !p.active() || p.role == Role::Goalkeeper {
            continue;
        }
        let d = frame.pos(i).distance(at);
        if d < da {
            (b, db) = (a, da);
            (a, da) = (Some(i), d);
        } else if d < db {
            (b, db) = (Some(i), d);
        }
    }
    (a, b)
}

fn nearest_to(s: &MatchState, frame: &TickFrame, side: Side, at: Vec2) -> Option<usize> {
    let mut best = None;
    let mut best_d = f32::MAX;
    for (i, p) in s.players.iter().enumerate() {
        if p.side != side || !p.active() || s.tick < p.touch_ready_tick {
            continue;
        }
        let d = frame.pos(i).distance(at);
        if d < best_d {
            best_d = d;
            best = Some(i);
        }
    }
    best
}

/// Off-the-play players re-plan every this many ticks (spec Fase 5).
const ARCADE_CADENCE: usize = 3;

fn move_players(
    s: &mut MatchState,
    frame: &TickFrame,
    targets: &[Vec2; PLAYERS],
    urgency: &[f32; PLAYERS],
    in_play: &[bool; PLAYERS],
) {
    let now = s.now_ms();
    let k = s.tuning.kinematics;
    // Every player re-plans every tick: one turn limit for all.
    #[allow(clippy::cast_precision_loss)] // 100
    let turn = TurnLimit::new(k.turn_rate, LOGICAL_DT_MS as f32 / 1000.0);
    let instant = PlayerKinematics::instant(&k, turn);
    for i in 0..PLAYERS {
        let p = &mut s.players[i];
        if p.sent_off {
            // Walks off: parked just outside the touchline.
            let at = Vec2::new(frame.pos(i).x.clamp(0.0, pitch::LENGTH), -3.0);
            p.traj = PlayerKinematics::plan_trajectory(at, at, 1.0, now);
            p.lead = None;
            continue;
        }
        let target = targets[i].clamp(
            Vec2::new(-2.0, -2.0),
            Vec2::new(pitch::LENGTH + 2.0, pitch::WIDTH + 2.0),
        );
        // From the cached position at `now`, with the movement physics
        // (instant velocity changes when the physics are off).
        let speed = p.top_speed * urgency[i];
        if instant {
            p.traj = PlayerKinematics::plan_trajectory(frame.pos(i), target, speed, now);
        } else if !in_play[i] {
            // Off the play: arcade model on a 3-tick cadence (staggered by
            // player), or at once when just leaving the play.
            let due = (s.tick as usize + i) % ARCADE_CADENCE == 0;
            if due || p.lead.is_some() {
                p.traj = PlayerKinematics::plan_trajectory(frame.pos(i), target, speed, now);
                p.lead = None;
            }
        } else {
            let (traj, lead) = PlayerKinematics::steer(
                (&p.traj, p.lead.as_ref()),
                frame.pos(i),
                target,
                speed,
                now,
                &k,
                turn,
            );
            p.traj = traj;
            p.lead = Some(lead);
        }
    }
}
