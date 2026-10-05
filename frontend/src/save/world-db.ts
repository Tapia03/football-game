// The world inside a save file (spec Fase 7B, 7B.3): what `world.create`,
// `world.load`, `world.commitDay`, `world.standings` and `world.round` do
// to the tables of schema v2. Each write is one transaction of the file.
//
// The blobs (sheets, dynamic states, line-ups, match seeds) are opaque
// here: their layouts belong to the Rust side (`fm-persistence`). This
// module only checks that they have the size the rows say.

import type { Database, SqlValue } from '@sqlite.org/sqlite-wasm';
import { OpError } from './errors';
import type {
  DayCommit,
  MatchResult,
  RoundMatch,
  StandingRow,
  WorldClub,
  WorldFixture,
  WorldPlayer,
  WorldSave,
} from './protocol';

/** Bytes of each blob, as `fm-persistence` lays them out. */
export const SHEET_BYTES = 60;
export const DYNAMIC_BYTES = 14;
export const LINEUP_BYTES = 44;
export const SEED_BYTES = 8;

/** The one competition of the MVP. */
const LEAGUE = { id: 1, name: 'Liga', kind: 0 };

const int = (v: SqlValue | undefined): number => (typeof v === 'number' ? v : Number(v ?? 0));
const text = (v: SqlValue | undefined): string => (typeof v === 'string' ? v : '');
const blob = (v: SqlValue | undefined): Uint8Array => (v instanceof Uint8Array ? v : new Uint8Array(0));
const nullable = (v: SqlValue | undefined): number | null => (v === null || v === undefined ? null : int(v));

function invalid(message: string): never {
  throw new OpError('invalid', message);
}

function meta(db: Database, key: string): SqlValue | undefined {
  return db.selectValue('SELECT value FROM meta WHERE key = ?', [key]);
}

function setMeta(db: Database, key: string, value: string | number): void {
  db.exec({
    sql: 'INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value',
    bind: [key, value],
  });
}

/** Whether the save has a world (a save of Fase 7A has none). */
export function hasWorld(db: Database): boolean {
  return meta(db, 'world_seed') !== undefined;
}

function needWorld(db: Database): void {
  if (!hasWorld(db)) {
    throw new OpError('no-world', 'este save não tem mundo; foi criado antes da Fase 7B');
  }
}

/** Runs `statement` once for each row of `rows`. */
function insertAll(db: Database, sql: string, rows: readonly (readonly (string | number | null | Uint8Array)[])[]): void {
  const statement = db.prepare(sql);
  try {
    for (const row of rows) statement.bind([...row]).stepReset();
  } finally {
    statement.finalize();
  }
}

/** Piece `index` of a buffer of pieces of `size` bytes (a copy). */
function piece(buffer: ArrayBuffer, index: number, size: number): Uint8Array {
  return new Uint8Array(buffer.slice(index * size, (index + 1) * size));
}

/** Writes a new world into a save that has none, in one transaction. */
export function createWorld(db: Database, world: WorldSave): void {
  if (hasWorld(db)) throw new OpError('world-exists', 'este save já tem um mundo');
  const { clubs, players, fixtures } = world;
  if (!/^\d{1,20}$/.test(world.seed)) invalid('a seed do mundo é um inteiro sem sinal em decimal');
  if (world.lineups.byteLength !== clubs.length * LINEUP_BYTES) invalid('um onze inicial por clube');
  if (world.sheets.byteLength !== players.length * SHEET_BYTES) invalid('uma ficha por jogador');
  if (world.dynamics.byteLength !== players.length * DYNAMIC_BYTES) invalid('um estado por jogador');
  if (world.matchSeeds.byteLength !== fixtures.length * SEED_BYTES) invalid('uma seed por partida');
  if (world.userClub < 0 || world.userClub >= clubs.length) invalid('o clube do usuário não é da liga');
  db.transaction(() => {
    insertAll(
      db,
      'INSERT INTO clubs (id, name, short_name, strength, formation) VALUES (?, ?, ?, ?, ?)',
      clubs.map((c, id) => [id, c.name, c.shortName, c.strength, c.formation]),
    );
    insertAll(
      db,
      `INSERT INTO tactics (club_id, formation, mentality, pressing, width, line_height, slots)
       VALUES (?, ?, ?, ?, ?, ?, ?)`,
      clubs.map((c, id) => [
        id,
        c.formation,
        c.mentality,
        c.pressing,
        c.width,
        c.lineHeight,
        piece(world.lineups, id, LINEUP_BYTES),
      ]),
    );
    insertAll(
      db,
      `INSERT INTO players (id, club_id, name, position, birth_year, overall, potential, condition, morale,
                            injury_weeks, sheet, dynamic)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      players.map((p, id) => [
        id,
        p.club,
        p.name,
        p.position,
        p.birthYear,
        p.overall,
        p.potential,
        p.condition,
        p.morale,
        p.injuryWeeks,
        piece(world.sheets, id, SHEET_BYTES),
        piece(world.dynamics, id, DYNAMIC_BYTES),
      ]),
    );
    db.exec({
      sql: 'INSERT INTO competitions (id, name, season, kind) VALUES (?, ?, ?, ?)',
      bind: [LEAGUE.id, LEAGUE.name, world.seasonYear, LEAGUE.kind],
    });
    insertAll(
      db,
      `INSERT INTO matches (id, competition_id, round, day, home_club_id, away_club_id, seed, played,
                            home_goals, away_goals, home_shots, away_shots, home_on_target, away_on_target)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      fixtures.map((f, id) => {
        const r = f.result;
        return [
          id,
          LEAGUE.id,
          f.round,
          f.day,
          f.home,
          f.away,
          piece(world.matchSeeds, id, SEED_BYTES),
          r === null ? 0 : 1,
          r?.homeGoals ?? null,
          r?.awayGoals ?? null,
          r?.homeShots ?? null,
          r?.awayShots ?? null,
          r?.homeOnTarget ?? null,
          r?.awayOnTarget ?? null,
        ];
      }),
    );
    setMeta(db, 'world_seed', world.seed);
    setMeta(db, 'day', world.day);
    setMeta(db, 'season_year', world.seasonYear);
    setMeta(db, 'user_club', world.userClub);
  });
}

/** Glues blobs of `size` bytes each into one buffer. */
function glue(parts: readonly Uint8Array[], size: number): ArrayBuffer {
  const out = new Uint8Array(parts.length * size);
  parts.forEach((part, i) => {
    if (part.length !== size) invalid(`blob de ${part.length} bytes onde cabem ${size}`);
    out.set(part, i * size);
  });
  return out.buffer;
}

/** The world of the save, as it was last committed. */
export function loadWorld(db: Database): WorldSave {
  needWorld(db);
  const clubRows = db.selectArrays(
    `SELECT c.name, c.short_name, c.strength, t.formation, t.mentality, t.pressing, t.width, t.line_height, t.slots
     FROM clubs c JOIN tactics t ON t.club_id = c.id ORDER BY c.id`,
  );
  const clubs: WorldClub[] = clubRows.map((r) => ({
    name: text(r[0]),
    shortName: text(r[1]),
    strength: int(r[2]),
    formation: int(r[3]),
    mentality: int(r[4]),
    pressing: int(r[5]),
    width: int(r[6]),
    lineHeight: int(r[7]),
  }));
  const playerRows = db.selectArrays(
    `SELECT club_id, name, position, birth_year, overall, potential, condition, morale, injury_weeks, sheet, dynamic
     FROM players ORDER BY id`,
  );
  const players: WorldPlayer[] = playerRows.map((r) => ({
    club: nullable(r[0]),
    name: text(r[1]),
    position: int(r[2]),
    birthYear: int(r[3]),
    overall: int(r[4]),
    potential: int(r[5]),
    condition: int(r[6]),
    morale: int(r[7]),
    injuryWeeks: int(r[8]),
  }));
  const matchRows = db.selectArrays(
    `SELECT round, day, home_club_id, away_club_id, played, home_goals, away_goals, home_shots, away_shots,
            home_on_target, away_on_target, seed
     FROM matches ORDER BY id`,
  );
  const fixtures: WorldFixture[] = matchRows.map((r) => ({
    round: int(r[0]),
    day: int(r[1]),
    home: int(r[2]),
    away: int(r[3]),
    result:
      int(r[4]) === 0
        ? null
        : {
            homeGoals: int(r[5]),
            awayGoals: int(r[6]),
            homeShots: int(r[7]),
            awayShots: int(r[8]),
            homeOnTarget: int(r[9]),
            awayOnTarget: int(r[10]),
          },
  }));
  return {
    seed: String(meta(db, 'world_seed') ?? ''),
    day: int(meta(db, 'day')),
    seasonYear: int(meta(db, 'season_year')),
    userClub: int(meta(db, 'user_club')),
    clubs,
    lineups: glue(
      clubRows.map((r) => blob(r[8])),
      LINEUP_BYTES,
    ),
    players,
    sheets: glue(
      playerRows.map((r) => blob(r[9])),
      SHEET_BYTES,
    ),
    dynamics: glue(
      playerRows.map((r) => blob(r[10])),
      DYNAMIC_BYTES,
    ),
    fixtures,
    matchSeeds: glue(
      matchRows.map((r) => blob(r[11])),
      SEED_BYTES,
    ),
  };
}

/**
 * Ends a day: the results of its matches, every player's state and the new
 * day, in one transaction. Only the day right after the stored one is
 * accepted — a commit sent twice, or out of order, changes nothing.
 *
 * `hold` (tests): the writes are made inside a transaction that is left
 * open, as if the worker died right before the commit.
 */
export function commitDay(db: Database, commit: DayCommit, hold = false): void {
  needWorld(db);
  const stored = int(meta(db, 'day'));
  if (commit.day !== stored + 1) {
    throw new OpError(
      'out-of-order',
      `o save está no dia ${stored}; só aceita o fim desse dia (dia ${stored + 1}), não o dia ${commit.day}`,
    );
  }
  const playerCount = int(db.selectValue('SELECT count(*) FROM players'));
  if (commit.players.length !== playerCount) invalid(`o mundo tem ${playerCount} jogadores`);
  if (commit.dynamics.byteLength !== playerCount * DYNAMIC_BYTES) invalid('um estado por jogador');
  // The matches of the day being closed, all of them and nothing else.
  const due = db.selectValues('SELECT id FROM matches WHERE day = ? AND played = 0 ORDER BY id', [stored]).map(int);
  const sent = commit.results.map((r) => r.id).sort((a, b) => a - b);
  if (due.length !== sent.length || due.some((id, i) => id !== sent[i])) {
    invalid(`o dia ${stored} tem ${due.length} partidas a gravar; vieram ${sent.length}`);
  }
  const transaction = (writes: () => void): void => {
    if (!hold) {
      db.transaction(writes);
      return;
    }
    db.exec('BEGIN IMMEDIATE');
    writes();
  };
  transaction(() => {
    insertAll(
      db,
      `UPDATE matches SET played = 1, home_goals = ?, away_goals = ?, home_shots = ?, away_shots = ?,
                          home_on_target = ?, away_on_target = ?
       WHERE id = ?`,
      commit.results.map((r) => [
        r.homeGoals,
        r.awayGoals,
        r.homeShots,
        r.awayShots,
        r.homeOnTarget,
        r.awayOnTarget,
        r.id,
      ]),
    );
    insertAll(
      db,
      'UPDATE players SET condition = ?, morale = ?, injury_weeks = ?, dynamic = ? WHERE id = ?',
      commit.players.map((p, id) => [
        p.condition,
        p.morale,
        p.injuryWeeks,
        piece(commit.dynamics, id, DYNAMIC_BYTES),
        id,
      ]),
    );
    setMeta(db, 'day', commit.day);
  });
}

/** The league table, from the results: points, goal difference, goals, id. */
export function standings(db: Database): StandingRow[] {
  needWorld(db);
  const rows = db.selectArrays(`
    WITH sides AS (
      SELECT home_club_id AS club, home_goals AS scored, away_goals AS conceded FROM matches WHERE played = 1
      UNION ALL
      SELECT away_club_id, away_goals, home_goals FROM matches WHERE played = 1
    ),
    tally AS (
      SELECT c.id AS id, c.name AS name, c.short_name AS short_name,
             count(s.club) AS played,
             coalesce(sum(s.scored > s.conceded), 0) AS won,
             coalesce(sum(s.scored = s.conceded), 0) AS drawn,
             coalesce(sum(s.scored < s.conceded), 0) AS lost,
             coalesce(sum(s.scored), 0) AS goals_for,
             coalesce(sum(s.conceded), 0) AS goals_against
      FROM clubs c LEFT JOIN sides s ON s.club = c.id
      GROUP BY c.id
    )
    SELECT id, name, short_name, played, won, drawn, lost, goals_for, goals_against, 3 * won + drawn AS points
    FROM tally
    ORDER BY points DESC, goals_for - goals_against DESC, goals_for DESC, id
  `);
  return rows.map((r) => ({
    club: int(r[0]),
    name: text(r[1]),
    shortName: text(r[2]),
    played: int(r[3]),
    won: int(r[4]),
    drawn: int(r[5]),
    lost: int(r[6]),
    goalsFor: int(r[7]),
    goalsAgainst: int(r[8]),
    points: int(r[9]),
  }));
}

/** The matches of round `round` (0-based), in id order. */
export function round(db: Database, which: number): RoundMatch[] {
  needWorld(db);
  return db
    .selectArrays(
      `SELECT id, day, home_club_id, away_club_id, played, home_goals, away_goals, home_shots, away_shots,
              home_on_target, away_on_target
       FROM matches WHERE round = ? ORDER BY id`,
      [which],
    )
    .map((r) => {
      const result: MatchResult | null =
        int(r[4]) === 0
          ? null
          : {
              homeGoals: int(r[5]),
              awayGoals: int(r[6]),
              homeShots: int(r[7]),
              awayShots: int(r[8]),
              homeOnTarget: int(r[9]),
              awayOnTarget: int(r[10]),
            };
      return { id: int(r[0]), day: int(r[1]), home: int(r[2]), away: int(r[3]), result };
    });
}

/** What the catalog shows about the save: club, season and day of its world. */
export function summary(db: Database): { clubName: string; season: number; day: number } | undefined {
  if (!hasWorld(db)) return undefined;
  return {
    clubName: text(db.selectValue('SELECT name FROM clubs WHERE id = ?', [int(meta(db, 'user_club'))])),
    season: int(meta(db, 'season_year')),
    day: int(meta(db, 'day')),
  };
}
