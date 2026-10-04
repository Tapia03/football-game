//! Test helpers: a demo match state with chosen players placed by hand.

use fm_core::Vec2;

use crate::kinematics::PlayerKinematics;
use crate::state::{BallState, MatchState};

/// Demo-match state at tick 10 (home attacks right). Players listed in
/// `placed` stand still at the given points; everyone else is parked along
/// the bottom touchline, far from play. The ball is held by `holder`.
pub fn placed_state(placed: &[(usize, Vec2)], holder: usize) -> MatchState {
    let (db, setup) = crate::demo::demo_match(5);
    let mut s = crate::engine::MatchEngine::new(&setup, &db).state().clone();
    s.tick = 10;
    let now = s.now_ms();
    for (i, p) in s.players.iter_mut().enumerate() {
        let parked = Vec2::new(5.0 + 2.0 * f32::from(u8::try_from(i).expect("< 22")), 1.0);
        let at = placed
            .iter()
            .find(|(j, _)| *j == i)
            .map_or(parked, |(_, at)| *at);
        p.traj = PlayerKinematics::plan_trajectory(at, at, 0.0, now);
        p.lead = None;
    }
    s.ball = BallState::Held {
        holder: u8::try_from(holder).expect("< 22"),
    };
    s
}
