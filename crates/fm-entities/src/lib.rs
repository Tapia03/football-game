//! Player database (`SoA`), attributes and the weekly update.
//!
//! `PlayerId(u32)` is the only key: no `Rc<Player>`/`Arc<Player>`. Cold data
//! (`PlayerStatic`) and hot data (`PlayerDynamic`) live in parallel `Vec`s.

pub mod attributes;
pub mod database;
pub mod generate;
pub mod player;

pub use attributes::{
    HiddenAttributes, MentalAttributes, PhysicalAttributes, PlayerAttributes, TechnicalAttributes,
};
pub use database::{dynamics_digest, rng_for_week, update_player, PlayerDatabase};
pub use generate::generate_database;
pub use player::{
    ClubId, Foot, InjuryKind, PlayerBio, PlayerDynamic, PlayerId, PlayerStatic, Position, BP_MAX,
    BP_NEUTRAL,
};
