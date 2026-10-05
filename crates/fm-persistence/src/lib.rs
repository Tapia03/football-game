//! Byte layouts of what a save keeps as blobs (spec Fase 7B, 7B.3): a
//! player's static sheet, a player's dynamic state and a club's starting
//! eleven. Explicit little-endian layouts with the layout version in the
//! first byte — never a memory image of a Rust struct — so a file written
//! today reads back bit for bit on any target and after any refactor.
//! Decoding validates: a bad blob is an error, never a crooked state.

#![forbid(unsafe_code)]

use fm_entities::{
    ClubId, Foot, GoalkeepingAttributes, HiddenAttributes, InjuryKind, MentalAttributes,
    PhysicalAttributes, PlayerAttributes, PlayerBio, PlayerDatabase, PlayerDynamic, PlayerId,
    PlayerStatic, Position, TechnicalAttributes,
};

/// Crate name, used by the workspace smoke test to prove the crate links.
pub const CRATE_NAME: &str = "fm-persistence";

/// Version of the layouts below (the first byte of every sheet and state).
pub const LAYOUT_VERSION: u8 = 1;
/// Bytes of a player's static sheet.
pub const SHEET_BYTES: usize = 60;
/// Bytes of a player's dynamic state.
pub const DYNAMIC_BYTES: usize = 14;
/// Bytes of a starting eleven: 11 player ids.
pub const LINEUP_BYTES: usize = 44;

/// `club` bytes of a player without a club.
const NO_CLUB: u32 = u32::MAX;

// Offsets inside a sheet.
const TECHNICAL: usize = 1;
const MENTAL: usize = TECHNICAL + 10;
const PHYSICAL: usize = MENTAL + 9;
const HIDDEN: usize = PHYSICAL + 8;
const GOALKEEPING: usize = HIDDEN + 8;
const BIO: usize = GOALKEEPING + 6;
const POSITION: usize = BIO + 12;
const POTENTIAL: usize = POSITION + 1;
const CLUB: usize = POTENTIAL + 1;

const _: () = assert!(BIO == 42 && CLUB + 4 == SHEET_BYTES);

/// Why a blob was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// Not the number of bytes the layout has.
    Length {
        expected: usize,
        found: usize,
    },
    /// A layout version this build does not know.
    Version(u8),
    /// A byte that is not a position, a foot or a kind of injury.
    Position(u8),
    Foot(u8),
    Injury(u8),
    /// An attribute or the potential outside 1..=100.
    Attributes,
    /// A dynamic state outside its ranges.
    Dynamic,
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Length { expected, found } => {
                write!(f, "blob with {found} bytes, expected {expected}")
            }
            Self::Version(v) => write!(f, "layout version {v}, this build knows {LAYOUT_VERSION}"),
            Self::Position(b) => write!(f, "byte {b} is not a position"),
            Self::Foot(b) => write!(f, "byte {b} is not a foot"),
            Self::Injury(b) => write!(f, "byte {b} is not a kind of injury"),
            Self::Attributes => write!(f, "attribute or potential outside 1..=100"),
            Self::Dynamic => write!(f, "dynamic state outside its ranges"),
        }
    }
}

impl std::error::Error for DecodeError {}

fn check(bytes: &[u8], expected: usize) -> Result<(), DecodeError> {
    if bytes.len() != expected {
        return Err(DecodeError::Length {
            expected,
            found: bytes.len(),
        });
    }
    if bytes[0] != LAYOUT_VERSION {
        return Err(DecodeError::Version(bytes[0]));
    }
    Ok(())
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

const fn foot_code(foot: Foot) -> u8 {
    match foot {
        Foot::Left => 0,
        Foot::Right => 1,
        Foot::Both => 2,
    }
}

const fn injury_code(kind: InjuryKind) -> u8 {
    match kind {
        InjuryKind::None => 0,
        InjuryKind::Knock => 1,
        InjuryKind::Strain => 2,
        InjuryKind::Tear => 3,
        InjuryKind::Major => 4,
    }
}

/// Code of a position in a sheet: its place in `Position::ALL`. Also what
/// the `position` column of the save holds.
///
/// # Panics
/// Never: every position is in `Position::ALL`.
#[must_use]
pub fn position_code(position: Position) -> u8 {
    let at = Position::ALL
        .iter()
        .position(|&p| p == position)
        .expect("every position is in Position::ALL");
    u8::try_from(at).expect("12 positions")
}

/// A player's static sheet as it goes into the save.
#[must_use]
pub fn encode_sheet(player: &PlayerStatic) -> [u8; SHEET_BYTES] {
    let mut out = [0_u8; SHEET_BYTES];
    out[0] = LAYOUT_VERSION;
    let a = &player.attributes;
    out[TECHNICAL..MENTAL].copy_from_slice(&a.technical.to_array());
    out[MENTAL..PHYSICAL].copy_from_slice(&a.mental.to_array());
    out[PHYSICAL..HIDDEN].copy_from_slice(&a.physical.to_array());
    out[HIDDEN..GOALKEEPING].copy_from_slice(&a.hidden.to_array());
    out[GOALKEEPING..BIO].copy_from_slice(&a.goalkeeping.to_array());
    let b = &player.bio;
    out[BIO..BIO + 2].copy_from_slice(&b.first_name.to_le_bytes());
    out[BIO + 2..BIO + 4].copy_from_slice(&b.last_name.to_le_bytes());
    out[BIO + 4..BIO + 6].copy_from_slice(&b.nationality.to_le_bytes());
    out[BIO + 6..BIO + 8].copy_from_slice(&b.birth_year.to_le_bytes());
    out[BIO + 8] = b.birth_week;
    out[BIO + 9] = b.height_cm;
    out[BIO + 10] = b.weight_kg;
    out[BIO + 11] = foot_code(b.foot);
    out[POSITION] = position_code(player.position);
    out[POTENTIAL] = player.potential;
    out[CLUB..].copy_from_slice(&player.club.map_or(NO_CLUB, |c| c.0).to_le_bytes());
    out
}

/// Reads a sheet back.
///
/// # Errors
/// When the blob is not a sheet of a known layout or holds values no
/// player can have.
pub fn decode_sheet(bytes: &[u8]) -> Result<PlayerStatic, DecodeError> {
    check(bytes, SHEET_BYTES)?;
    // Raw values: `from_fn` would clamp a bad byte into range, and a bad
    // byte must be refused, not repaired.
    if bytes[TECHNICAL..BIO].iter().any(|v| !(1..=100).contains(v)) {
        return Err(DecodeError::Attributes);
    }
    let attributes = PlayerAttributes {
        technical: TechnicalAttributes::from_fn(|i| bytes[TECHNICAL + i]),
        mental: MentalAttributes::from_fn(|i| bytes[MENTAL + i]),
        physical: PhysicalAttributes::from_fn(|i| bytes[PHYSICAL + i]),
        hidden: HiddenAttributes::from_fn(|i| bytes[HIDDEN + i]),
        goalkeeping: GoalkeepingAttributes::from_fn(|i| bytes[GOALKEEPING + i]),
    };
    let foot = match bytes[BIO + 11] {
        0 => Foot::Left,
        1 => Foot::Right,
        2 => Foot::Both,
        other => return Err(DecodeError::Foot(other)),
    };
    let position = *Position::ALL
        .get(usize::from(bytes[POSITION]))
        .ok_or(DecodeError::Position(bytes[POSITION]))?;
    let potential = bytes[POTENTIAL];
    if !(1..=100).contains(&potential) {
        return Err(DecodeError::Attributes);
    }
    let club = u32_at(bytes, CLUB);
    Ok(PlayerStatic {
        attributes,
        bio: PlayerBio {
            first_name: u16_at(bytes, BIO),
            last_name: u16_at(bytes, BIO + 2),
            nationality: u16_at(bytes, BIO + 4),
            birth_year: u16_at(bytes, BIO + 6),
            birth_week: bytes[BIO + 8],
            height_cm: bytes[BIO + 9],
            weight_kg: bytes[BIO + 10],
            foot,
        },
        position,
        potential,
        club: (club != NO_CLUB).then_some(ClubId(club)),
    })
}

/// A player's dynamic state as it goes into the save.
#[must_use]
pub fn encode_dynamic(state: &PlayerDynamic) -> [u8; DYNAMIC_BYTES] {
    let mut out = [0_u8; DYNAMIC_BYTES];
    out[0] = LAYOUT_VERSION;
    out[1..3].copy_from_slice(&state.fatigue.to_le_bytes());
    out[3..5].copy_from_slice(&state.condition.to_le_bytes());
    out[5..7].copy_from_slice(&state.form.to_le_bytes());
    out[7..9].copy_from_slice(&state.morale.to_le_bytes());
    out[9..11].copy_from_slice(&state.minutes_this_week.to_le_bytes());
    out[11] = state.injury_weeks;
    out[12] = injury_code(state.injury_kind);
    out[13] = state.weeks_unused;
    out
}

/// Reads a dynamic state back.
///
/// # Errors
/// When the blob is not a state of a known layout or is outside its ranges.
pub fn decode_dynamic(bytes: &[u8]) -> Result<PlayerDynamic, DecodeError> {
    check(bytes, DYNAMIC_BYTES)?;
    let injury_kind = match bytes[12] {
        0 => InjuryKind::None,
        1 => InjuryKind::Knock,
        2 => InjuryKind::Strain,
        3 => InjuryKind::Tear,
        4 => InjuryKind::Major,
        other => return Err(DecodeError::Injury(other)),
    };
    let state = PlayerDynamic {
        fatigue: u16_at(bytes, 1),
        condition: u16_at(bytes, 3),
        form: u16_at(bytes, 5),
        morale: u16_at(bytes, 7),
        minutes_this_week: u16_at(bytes, 9),
        injury_weeks: bytes[11],
        injury_kind,
        weeks_unused: bytes[13],
    };
    if state.is_valid() {
        Ok(state)
    } else {
        Err(DecodeError::Dynamic)
    }
}

/// A starting eleven as it goes into the save: the 11 ids, slot by slot.
/// (No version byte: it is a list of ids and nothing else.)
#[must_use]
pub fn encode_lineup(lineup: &[PlayerId; 11]) -> [u8; LINEUP_BYTES] {
    let mut out = [0_u8; LINEUP_BYTES];
    for (chunk, id) in out.chunks_exact_mut(4).zip(lineup) {
        chunk.copy_from_slice(&id.0.to_le_bytes());
    }
    out
}

/// Reads a starting eleven back.
///
/// # Errors
/// When the blob is not 44 bytes.
pub fn decode_lineup(bytes: &[u8]) -> Result<[PlayerId; 11], DecodeError> {
    if bytes.len() != LINEUP_BYTES {
        return Err(DecodeError::Length {
            expected: LINEUP_BYTES,
            found: bytes.len(),
        });
    }
    let mut out = [PlayerId(0); 11];
    for (id, chunk) in out.iter_mut().zip(bytes.chunks_exact(4)) {
        *id = PlayerId(u32_at(chunk, 0));
    }
    Ok(out)
}

/// The sheets of every player of `players`, one after the other, in id
/// order.
#[must_use]
pub fn encode_sheets(players: &PlayerDatabase) -> Vec<u8> {
    players.statics().iter().flat_map(encode_sheet).collect()
}

/// The dynamic states of every player of `players`, in id order.
#[must_use]
pub fn encode_dynamics(players: &PlayerDatabase) -> Vec<u8> {
    players.dynamics().iter().flat_map(encode_dynamic).collect()
}

/// The database `encode_sheets` and `encode_dynamics` came from.
///
/// # Errors
/// When the two blobs are not whole lists of the same number of players,
/// or any player in them is refused.
pub fn decode_database(sheets: &[u8], dynamics: &[u8]) -> Result<PlayerDatabase, DecodeError> {
    let count = sheets.len() / SHEET_BYTES;
    if sheets.len() != count * SHEET_BYTES {
        return Err(DecodeError::Length {
            expected: count * SHEET_BYTES,
            found: sheets.len(),
        });
    }
    if dynamics.len() != count * DYNAMIC_BYTES {
        return Err(DecodeError::Length {
            expected: count * DYNAMIC_BYTES,
            found: dynamics.len(),
        });
    }
    let mut players = PlayerDatabase::with_capacity(count);
    let rows = sheets
        .chunks_exact(SHEET_BYTES)
        .zip(dynamics.chunks_exact(DYNAMIC_BYTES));
    for (sheet, state) in rows {
        let id = players.create(decode_sheet(sheet)?);
        *players.dynamic_mut(id) = decode_dynamic(state)?;
    }
    Ok(players)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_entities::generate_database;

    /// A database that has lived: clubs set, weeks played, some injuries.
    fn lived_in() -> PlayerDatabase {
        let mut db = generate_database(500, 7, 2026);
        for i in 0..500_u32 {
            let id = PlayerId(i);
            db.static_mut(id).club = (i % 7 != 0).then_some(ClubId(i / 25));
        }
        for week in 1..=30 {
            for i in (0..500_u32).step_by(2) {
                db.record_minutes(PlayerId(i), 90);
            }
            db.weekly_update(99, week);
        }
        db
    }

    /// 500 players, encoded and decoded: the same database, bit for bit —
    /// one by one and as whole lists.
    #[test]
    fn round_trip_is_exact() {
        let db = lived_in();
        assert!(
            db.dynamics().iter().any(|d| d.injury_weeks > 0),
            "somebody is injured"
        );
        assert!(db.statics().iter().any(|s| s.club.is_none()));
        for (s, d) in db.statics().iter().zip(db.dynamics()) {
            assert_eq!(decode_sheet(&encode_sheet(s)), Ok(*s));
            assert_eq!(decode_dynamic(&encode_dynamic(d)), Ok(*d));
        }
        let (sheets, dynamics) = (encode_sheets(&db), encode_dynamics(&db));
        assert_eq!(sheets.len(), 500 * SHEET_BYTES);
        assert_eq!(dynamics.len(), 500 * DYNAMIC_BYTES);
        assert_eq!(decode_database(&sheets, &dynamics), Ok(db));
    }

    /// The layout is pinned: this exact player is these exact bytes. A
    /// change here is a change of layout and needs a new `LAYOUT_VERSION`.
    #[test]
    fn layout_is_pinned() {
        let player = PlayerStatic {
            attributes: PlayerAttributes {
                technical: TechnicalAttributes::from_fn(|i| 10 + u8::try_from(i).unwrap()),
                mental: MentalAttributes::from_fn(|i| 30 + u8::try_from(i).unwrap()),
                physical: PhysicalAttributes::from_fn(|i| 50 + u8::try_from(i).unwrap()),
                hidden: HiddenAttributes::from_fn(|i| 70 + u8::try_from(i).unwrap()),
                goalkeeping: GoalkeepingAttributes::from_fn(|i| 90 + u8::try_from(i).unwrap()),
            },
            bio: PlayerBio {
                first_name: 0x0102,
                last_name: 0x0304,
                nationality: 0x0506,
                birth_year: 2001,
                birth_week: 33,
                height_cm: 181,
                weight_kg: 77,
                foot: Foot::Both,
            },
            position: Position::Striker,
            potential: 88,
            club: Some(ClubId(0x0A0B_0C0D)),
        };
        let expected: [u8; SHEET_BYTES] = [
            1, // version
            10, 11, 12, 13, 14, 15, 16, 17, 18, 19, // technical
            30, 31, 32, 33, 34, 35, 36, 37, 38, // mental
            50, 51, 52, 53, 54, 55, 56, 57, // physical
            70, 71, 72, 73, 74, 75, 76, 77, // hidden
            90, 91, 92, 93, 94, 95, // goalkeeping
            0x02, 0x01, 0x04, 0x03, 0x06, 0x05, 0xD1, 0x07, // names, nationality, 2001
            33, 181, 77, 2, // week, height, weight, both feet
            11, 88, // striker, potential
            0x0D, 0x0C, 0x0B, 0x0A, // club
        ];
        assert_eq!(encode_sheet(&player), expected);
        let state = PlayerDynamic {
            fatigue: 0x0102,
            condition: 9_000,
            form: 5_000,
            morale: 4_321,
            minutes_this_week: 180,
            injury_weeks: 3,
            injury_kind: InjuryKind::Strain,
            weeks_unused: 2,
        };
        assert_eq!(
            encode_dynamic(&state),
            [1, 0x02, 0x01, 0x28, 0x23, 0x88, 0x13, 0xE1, 0x10, 180, 0, 3, 2, 2]
        );
        let lineup: [PlayerId; 11] =
            core::array::from_fn(|i| PlayerId(u32::try_from(i).unwrap() * 257));
        let bytes = encode_lineup(&lineup);
        assert_eq!(bytes[..8], [0, 0, 0, 0, 1, 1, 0, 0]);
        assert_eq!(decode_lineup(&bytes), Ok(lineup));
        // No club is its own value, and comes back as none.
        let free = PlayerStatic {
            club: None,
            ..player
        };
        assert_eq!(encode_sheet(&free)[CLUB..], [0xFF; 4]);
        assert_eq!(decode_sheet(&encode_sheet(&free)), Ok(free));
    }

    /// A bad blob is refused with the reason, never repaired.
    #[test]
    fn corrupted_blobs_are_refused() {
        let db = lived_in();
        let sheet = encode_sheet(&db.statics()[0]);
        let state = encode_dynamic(&db.dynamics()[0]);
        let with = |at: usize, value: u8| {
            let mut bytes = sheet;
            bytes[at] = value;
            decode_sheet(&bytes)
        };
        assert_eq!(with(0, 2), Err(DecodeError::Version(2)));
        assert_eq!(with(0, 0), Err(DecodeError::Version(0)));
        assert_eq!(with(TECHNICAL + 3, 0), Err(DecodeError::Attributes));
        assert_eq!(with(GOALKEEPING + 5, 101), Err(DecodeError::Attributes));
        assert_eq!(with(POTENTIAL, 0), Err(DecodeError::Attributes));
        assert_eq!(with(POSITION, 12), Err(DecodeError::Position(12)));
        assert_eq!(with(BIO + 11, 3), Err(DecodeError::Foot(3)));
        assert_eq!(
            decode_sheet(&sheet[..59]),
            Err(DecodeError::Length {
                expected: 60,
                found: 59
            })
        );
        assert_eq!(
            decode_sheet(&[]),
            Err(DecodeError::Length {
                expected: 60,
                found: 0
            })
        );

        let mut bad = state;
        bad[12] = 5;
        assert_eq!(decode_dynamic(&bad), Err(DecodeError::Injury(5)));
        let mut bad = state;
        bad[0] = 9;
        assert_eq!(decode_dynamic(&bad), Err(DecodeError::Version(9)));
        // Condition above the fixed-point scale (10 000).
        let mut bad = state;
        bad[3..5].copy_from_slice(&20_000_u16.to_le_bytes());
        assert_eq!(decode_dynamic(&bad), Err(DecodeError::Dynamic));
        assert_eq!(
            decode_dynamic(&state[..13]),
            Err(DecodeError::Length {
                expected: 14,
                found: 13
            })
        );
        assert_eq!(
            decode_lineup(&[0; 43]),
            Err(DecodeError::Length {
                expected: 44,
                found: 43
            })
        );

        // Whole lists: a ragged blob, a mismatch of counts, one bad player.
        let (sheets, dynamics) = (encode_sheets(&db), encode_dynamics(&db));
        assert!(matches!(
            decode_database(&sheets[..sheets.len() - 1], &dynamics),
            Err(DecodeError::Length { .. })
        ));
        assert!(matches!(
            decode_database(&sheets, &dynamics[DYNAMIC_BYTES..]),
            Err(DecodeError::Length { .. })
        ));
        let mut one_bad = sheets.clone();
        one_bad[SHEET_BYTES * 250] = 7;
        assert_eq!(
            decode_database(&one_bad, &dynamics),
            Err(DecodeError::Version(7))
        );
        assert_eq!(
            DecodeError::Version(7).to_string(),
            "layout version 7, this build knows 1"
        );
    }
}
