//! The world of the game (spec Fase 7B): one league of 20 clubs and 500
//! synthetic players, its calendar, and the simulation of each day with the
//! existing match engine in LOD `Abstract`. Everything is a deterministic
//! function of the world seed; nothing here changes the engine.

#![forbid(unsafe_code)]

pub mod fixtures;
pub mod names;
pub mod squad;
pub mod world;

pub use fixtures::{
    day_of_round, season_fixtures, Fixture, MatchResult, CLUBS, MATCHES, MATCHES_PER_ROUND, ROUNDS,
    SEASON_DAYS,
};
pub use names::{player_name, CLUB_NAMES};
pub use squad::{fits, overall, pick_lineup};
pub use world::{
    Club, Played, Standing, World, LEAGUE_FORMATION, PLAYERS, SEASON_YEAR, SQUAD_SIZE,
};

/// Crate name, used by the workspace smoke test to prove the crate links.
pub const CRATE_NAME: &str = "fm-world";
