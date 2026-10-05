//! The world as the save holds it (spec Fase 7B, 7B.3): plain data — rows
//! and blobs — with `World::to_save` and `World::from_save` between the
//! two. Besides what the engine needs back bit for bit (the blobs of
//! `fm-persistence`), it carries the columns the screens query, worked out
//! here so that nobody outside Rust has to know how.

use fm_entities::{ClubId, PlayerId};
use fm_match::{Formation, LineHeight, Mentality, Pressing, Tactics, Width};
use fm_persistence::{
    decode_database, decode_lineup, encode_dynamics, encode_lineup, encode_sheets, position_code,
    DecodeError, LINEUP_BYTES,
};

use crate::fixtures::{season_fixtures, Fixture, CLUBS, MATCHES};
use crate::names::{player_name, CLUB_NAMES};
use crate::squad::overall;
use crate::world::{Club, World, PLAYERS, SEASON_YEAR};

/// A club as the save holds it. Its id is its place in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClubRow {
    pub name: String,
    pub short_name: String,
    pub strength: u8,
    /// Codes: the place of each level in its list (`Formation::ALL`,
    /// Defensive..=Attacking, Low..=UltraHigh, Narrow..=Wide, Deep..=High).
    pub formation: u8,
    pub mentality: u8,
    pub pressing: u8,
    pub width: u8,
    pub line_height: u8,
    /// The starting eleven (`fm_persistence::encode_lineup`).
    pub lineup: [u8; LINEUP_BYTES],
}

/// What the screens query about a player; the player's id is its place in
/// the list. Everything here can be worked out again from the blobs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerRow {
    pub club: Option<u8>,
    pub name: String,
    /// `fm_persistence::position_code`.
    pub position: u8,
    pub birth_year: u16,
    pub overall: u8,
    pub potential: u8,
    /// Basis points, 0..=10 000.
    pub condition: u16,
    pub morale: u16,
    pub injury_weeks: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldSave {
    pub seed: u64,
    pub day: u16,
    pub season_year: u16,
    pub user_club: u8,
    pub clubs: Vec<ClubRow>,
    pub players: Vec<PlayerRow>,
    /// Every player's static sheet, in id order (`encode_sheets`).
    pub sheets: Vec<u8>,
    /// Every player's dynamic state, in id order (`encode_dynamics`).
    pub dynamics: Vec<u8>,
    /// The season's matches, in id order, with the results so far.
    pub fixtures: Vec<Fixture>,
}

/// Why a save is not a world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveError {
    /// A blob was refused.
    Blob(DecodeError),
    /// The rows do not have the shape of a world: what is wrong.
    Shape(&'static str),
}

impl core::fmt::Display for SaveError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Blob(e) => write!(f, "{e}"),
            Self::Shape(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<DecodeError> for SaveError {
    fn from(e: DecodeError) -> Self {
        Self::Blob(e)
    }
}

const MENTALITIES: [Mentality; 5] = [
    Mentality::Defensive,
    Mentality::Cautious,
    Mentality::Balanced,
    Mentality::Positive,
    Mentality::Attacking,
];
const PRESSINGS: [Pressing; 4] = [
    Pressing::Low,
    Pressing::Medium,
    Pressing::High,
    Pressing::UltraHigh,
];
const WIDTHS: [Width; 3] = [Width::Narrow, Width::Normal, Width::Wide];
const LINE_HEIGHTS: [LineHeight; 3] = [LineHeight::Deep, LineHeight::Normal, LineHeight::High];

/// Place of `value` in `list` (the lists above are exhaustive).
fn code<T: PartialEq>(list: &[T], value: &T) -> u8 {
    list.iter()
        .position(|v| v == value)
        .and_then(|at| u8::try_from(at).ok())
        .unwrap_or(0)
}

fn from_code<T: Copy>(list: &[T], at: u8, what: &'static str) -> Result<T, SaveError> {
    list.get(usize::from(at))
        .copied()
        .ok_or(SaveError::Shape(what))
}

impl World {
    /// The world as rows and blobs.
    #[must_use]
    pub fn to_save(&self) -> WorldSave {
        let clubs = self
            .clubs
            .iter()
            .zip(CLUB_NAMES)
            .map(|(club, (name, short_name))| ClubRow {
                name: name.to_owned(),
                short_name: short_name.to_owned(),
                strength: club.strength,
                formation: code(&Formation::ALL, &club.formation),
                mentality: code(&MENTALITIES, &club.tactics.mentality),
                pressing: code(&PRESSINGS, &club.tactics.pressing),
                width: code(&WIDTHS, &club.tactics.width),
                line_height: code(&LINE_HEIGHTS, &club.tactics.line_height),
                lineup: encode_lineup(&club.lineup),
            })
            .collect();
        let players = self
            .players
            .statics()
            .iter()
            .zip(self.players.dynamics())
            .map(|(s, d)| PlayerRow {
                club: s.club.and_then(|c| u8::try_from(c.0).ok()),
                name: player_name(s.bio.first_name, s.bio.last_name),
                position: position_code(s.position),
                birth_year: s.bio.birth_year,
                overall: overall(s),
                potential: s.potential,
                condition: d.condition,
                morale: d.morale,
                injury_weeks: d.injury_weeks,
            })
            .collect();
        WorldSave {
            seed: self.seed,
            day: self.day,
            season_year: SEASON_YEAR,
            user_club: self.user_club,
            clubs,
            players,
            sheets: encode_sheets(&self.players),
            dynamics: encode_dynamics(&self.players),
            fixtures: self.fixtures.clone(),
        }
    }

    /// The world a save holds. What the engine needs comes from the blobs
    /// and from the rows that have no blob (tactics, line-ups, results);
    /// the display columns of the players are not read — they are derived.
    ///
    /// # Errors
    /// When a blob is refused or the rows are not a world of this league:
    /// the wrong number of clubs, players or matches, a code out of range,
    /// a line-up with somebody from another club, or a calendar that is
    /// not the calendar of the world's seed.
    pub fn from_save(save: &WorldSave) -> Result<Self, SaveError> {
        if save.clubs.len() != CLUBS {
            return Err(SaveError::Shape("the league has 20 clubs"));
        }
        if save.fixtures.len() != MATCHES {
            return Err(SaveError::Shape("the season has 380 matches"));
        }
        if usize::from(save.user_club) >= CLUBS {
            return Err(SaveError::Shape(
                "the user's club is not a club of the league",
            ));
        }
        let players = decode_database(&save.sheets, &save.dynamics)?;
        if players.len() != PLAYERS {
            return Err(SaveError::Shape("the world has 500 players"));
        }
        // The calendar is a function of the seed: only the results are data.
        let calendar = season_fixtures(save.seed);
        let matches = |a: &Fixture, b: &Fixture| {
            (a.round, a.day, a.home, a.away, a.seed) == (b.round, b.day, b.home, b.away, b.seed)
        };
        if !save
            .fixtures
            .iter()
            .zip(&calendar)
            .all(|(a, b)| matches(a, b))
        {
            return Err(SaveError::Shape(
                "the calendar is not the calendar of the world's seed",
            ));
        }
        let mut clubs = Vec::with_capacity(CLUBS);
        for (c, row) in (0_u32..).zip(&save.clubs) {
            let id = ClubId(c);
            let squad: Vec<PlayerId> = players
                .ids()
                .filter(|&p| players.static_of(p).club == Some(id))
                .collect();
            let lineup = decode_lineup(&row.lineup)?;
            if !lineup.iter().all(|p| squad.contains(p)) {
                return Err(SaveError::Shape("a line-up has a player from another club"));
            }
            clubs.push(Club {
                id,
                strength: row.strength,
                formation: from_code(&Formation::ALL, row.formation, "unknown formation")?,
                tactics: Tactics {
                    mentality: from_code(&MENTALITIES, row.mentality, "unknown mentality")?,
                    pressing: from_code(&PRESSINGS, row.pressing, "unknown pressing")?,
                    width: from_code(&WIDTHS, row.width, "unknown width")?,
                    line_height: from_code(&LINE_HEIGHTS, row.line_height, "unknown line height")?,
                },
                lineup,
                squad,
            });
        }
        Ok(Self {
            seed: save.seed,
            day: save.day,
            user_club: save.user_club,
            players,
            clubs,
            fixtures: save.fixtures.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lived(seed: u64, days: u16) -> World {
        let mut world = World::new(seed, 5);
        for _ in 0..days {
            world.advance_day(|_, _| {});
        }
        world
    }

    /// Ten days, save, load, ten more: the same world as twenty days
    /// straight — bit for bit, results and players alike.
    #[test]
    fn saving_and_loading_changes_nothing() {
        let half = lived(7, 10);
        let save = half.to_save();
        let mut resumed = World::from_save(&save).expect("a world");
        assert_eq!(resumed, half, "what was saved is what comes back");
        for _ in 0..10 {
            resumed.advance_day(|_, _| {});
        }
        let straight = lived(7, 20);
        assert_eq!(resumed, straight);
        assert_eq!(resumed.to_save(), straight.to_save());
        assert_eq!(straight.day, 20);
        assert_eq!(
            straight
                .fixtures
                .iter()
                .filter(|f| f.result.is_some())
                .count(),
            20
        );
    }

    /// The rows say what the screens need, and agree with the blobs.
    #[test]
    fn rows_describe_the_world() {
        let world = lived(7, 8);
        let save = world.to_save();
        assert_eq!((save.seed, save.day, save.user_club), (7, 8, 5));
        assert_eq!(save.season_year, SEASON_YEAR);
        assert_eq!((save.clubs.len(), save.players.len()), (CLUBS, PLAYERS));
        assert_eq!(save.sheets.len(), PLAYERS * fm_persistence::SHEET_BYTES);
        assert_eq!(save.dynamics.len(), PLAYERS * fm_persistence::DYNAMIC_BYTES);
        assert_eq!(save.clubs[0].name, "Atlético Aurora");
        assert_eq!(save.clubs[0].short_name, "AUR");
        // Every club on 4-4-2 (code 0) with default tactics.
        assert!(save.clubs.iter().all(|c| (
            c.formation,
            c.mentality,
            c.pressing,
            c.width,
            c.line_height
        ) == (0, 2, 1, 1, 1)));
        for (id, row) in (0_u32..).zip(&save.players) {
            let s = world.players.static_of(PlayerId(id));
            let d = world.players.dynamic_of(PlayerId(id));
            assert_eq!(row.club, Some(u8::try_from(id / 25).unwrap()));
            assert_eq!(row.name, player_name(s.bio.first_name, s.bio.last_name));
            assert!(row.name.contains(' '));
            assert_eq!((row.overall, row.potential), (overall(s), s.potential));
            assert_eq!(
                (row.condition, row.morale, row.injury_weeks),
                (d.condition, d.morale, d.injury_weeks)
            );
            assert_eq!(row.birth_year, s.bio.birth_year);
            assert!(row.position < 12);
        }
        // A week went by: not everybody is as fresh as on day 0.
        assert!(save.players.iter().any(|p| p.condition < 10_000));
    }

    /// A save that is not a world of this league is refused, saying why.
    #[test]
    fn broken_saves_are_refused() {
        let good = lived(7, 7).to_save();
        let broken = |change: &dyn Fn(&mut WorldSave)| {
            let mut save = good.clone();
            change(&mut save);
            World::from_save(&save)
        };
        assert!(World::from_save(&good).is_ok());
        assert_eq!(
            broken(&|s| s.sheets[0] = 9),
            Err(SaveError::Blob(DecodeError::Version(9)))
        );
        assert!(matches!(
            broken(&|s| s.dynamics.truncate(100)),
            Err(SaveError::Blob(DecodeError::Length { .. }))
        ));
        assert_eq!(
            broken(&|s| {
                s.clubs.pop();
            }),
            Err(SaveError::Shape("the league has 20 clubs"))
        );
        assert_eq!(
            broken(&|s| {
                s.fixtures.pop();
            }),
            Err(SaveError::Shape("the season has 380 matches"))
        );
        assert_eq!(
            broken(&|s| s.user_club = 20),
            Err(SaveError::Shape(
                "the user's club is not a club of the league"
            ))
        );
        assert_eq!(
            broken(&|s| s.clubs[3].formation = 5),
            Err(SaveError::Shape("unknown formation"))
        );
        assert_eq!(
            broken(&|s| s.clubs[3].pressing = 4),
            Err(SaveError::Shape("unknown pressing"))
        );
        // Another world's seed: its calendar has other match seeds.
        assert_eq!(
            broken(&|s| s.seed = 8),
            Err(SaveError::Shape(
                "the calendar is not the calendar of the world's seed"
            ))
        );
        assert_eq!(
            broken(&|s| s.fixtures[40].home = (s.fixtures[40].home + 1) % 20),
            Err(SaveError::Shape(
                "the calendar is not the calendar of the world's seed"
            ))
        );
        // Club 0's line-up with a player of club 1.
        assert_eq!(
            broken(&|s| s.clubs[0].lineup[..4].copy_from_slice(&30_u32.to_le_bytes())),
            Err(SaveError::Shape("a line-up has a player from another club"))
        );
    }
}
