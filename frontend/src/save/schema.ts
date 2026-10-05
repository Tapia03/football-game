// Schemas of the two kinds of file the database worker keeps (spec Fase
// 7A): the catalog (one, the list of saves) and a save (one file each).
//
// A schema is a chain of migrations: version N of a file is what you get by
// applying migrations 1..=N to an empty file. The first save is v1; a later
// phase that adds tables only appends functions here. Nothing in a released
// migration may ever change — files in the wild already ran it.

/** What a migration may do to the file it is upgrading. */
export type MigrationDb = {
  exec(sql: string): void;
  /** The single number a query answers with (a count, a PRAGMA). */
  value(sql: string): number;
};

export type Migration = {
  /** Version the file is at once this migration has run (1, 2, 3, …). */
  readonly version: number;
  readonly name: string;
  readonly up: (db: MigrationDb) => void;
};

/**
 * `PRAGMA application_id` of our files ("FMsv" in ASCII): how an imported
 * file is recognised as one of ours.
 */
export const APPLICATION_ID = 0x464d_7376;

/** Every file records the migrations it went through. */
const MIGRATIONS_TABLE = `
  CREATE TABLE migrations (
    version    INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    applied_at TEXT NOT NULL
  ) STRICT;
`;

export const CATALOG_MIGRATIONS: readonly Migration[] = [
  {
    version: 1,
    name: 'catalog',
    up: (db) =>
      db.exec(`
        ${MIGRATIONS_TABLE}
        CREATE TABLE saves (
          id             TEXT PRIMARY KEY,
          name           TEXT NOT NULL,
          created_at     TEXT NOT NULL,
          last_opened_at TEXT NOT NULL,
          club_name      TEXT NOT NULL DEFAULT '',
          season         INTEGER NOT NULL DEFAULT 0,
          day            INTEGER NOT NULL DEFAULT 0,
          -- The file of the pool that holds the save right now. Replacing
          -- a save swaps this pointer in one transaction.
          active_file    TEXT NOT NULL,
          schema_version INTEGER NOT NULL
        ) STRICT;
      `),
  },
];

// Columns the screens query are real columns; what the engine needs back
// bit for bit (attribute blocks, dynamic state) is a BLOB of the bytes it
// has in memory.
export const SAVE_MIGRATIONS: readonly Migration[] = [
  {
    version: 1,
    name: 'world',
    up: (db) =>
      db.exec(`
        ${MIGRATIONS_TABLE}
        -- World seed, current day, season, the user's club…
        CREATE TABLE meta (
          key   TEXT PRIMARY KEY,
          value ANY
        ) STRICT, WITHOUT ROWID;

        CREATE TABLE clubs (
          id         INTEGER PRIMARY KEY,
          name       TEXT NOT NULL,
          short_name TEXT NOT NULL
        ) STRICT;

        CREATE TABLE players (
          id         INTEGER PRIMARY KEY,
          club_id    INTEGER REFERENCES clubs (id),
          name       TEXT NOT NULL,
          position   INTEGER NOT NULL,
          birth_year INTEGER NOT NULL,
          overall    INTEGER NOT NULL,
          condition  INTEGER NOT NULL,
          morale     INTEGER NOT NULL,
          -- The five attribute blocks (41 bytes) and the dynamic state
          -- (14 bytes), exactly as the engine holds them.
          attributes BLOB NOT NULL,
          dynamic    BLOB NOT NULL
        ) STRICT;
        CREATE INDEX players_by_club ON players (club_id);

        CREATE TABLE competitions (
          id     INTEGER PRIMARY KEY,
          name   TEXT NOT NULL,
          season INTEGER NOT NULL,
          kind   INTEGER NOT NULL
        ) STRICT;

        CREATE TABLE matches (
          id             INTEGER PRIMARY KEY,
          competition_id INTEGER NOT NULL REFERENCES competitions (id),
          round          INTEGER NOT NULL,
          day            INTEGER NOT NULL,
          home_club_id   INTEGER NOT NULL REFERENCES clubs (id),
          away_club_id   INTEGER NOT NULL REFERENCES clubs (id),
          seed           INTEGER NOT NULL,
          played         INTEGER NOT NULL DEFAULT 0,
          home_goals     INTEGER,
          away_goals     INTEGER
        ) STRICT;
        CREATE INDEX matches_by_round ON matches (competition_id, round);
        CREATE INDEX matches_by_day ON matches (day);

        CREATE TABLE tactics (
          club_id     INTEGER PRIMARY KEY REFERENCES clubs (id),
          formation   INTEGER NOT NULL,
          mentality   INTEGER NOT NULL,
          pressing    INTEGER NOT NULL,
          width       INTEGER NOT NULL,
          line_height INTEGER NOT NULL,
          -- Player and role of each of the 11 slots.
          slots       BLOB NOT NULL
        ) STRICT;
      `),
  },
];

/** Tables v2 recreates: empty, by design, in every v1 file. */
const RECREATED_IN_V2 = ['players', 'clubs', 'matches'] as const;

/**
 * v2 (Fase 7B): the world. `players`, `clubs` and `matches` are recreated
 * in their final shape rather than altered column by column — Fase 7A
 * never wrote to them, so every v1 file has them empty, and recreating
 * gives NOT NULL where it belongs and leaves no dead column.
 *
 * The guard: if any of the three has rows, this is not a v1 file as the
 * game wrote it (a bug, corruption, older code). The migration fails,
 * naming each table and how many rows it had; the chain rolls back and the
 * file stays as it was.
 */
function worldV2(db: MigrationDb): void {
  const rows = RECREATED_IN_V2.map((table) => ({
    table,
    // Table names are the constants above, never input.
    count: db.value(`SELECT count(*) FROM ${table}`),
  })).filter((t) => t.count > 0);
  if (rows.length > 0) {
    const found = rows.map((t) => `${t.table}: ${t.count} ${t.count === 1 ? 'linha' : 'linhas'}`);
    throw new Error(
      `a v2 recria players, clubs e matches, que deviam estar vazias num save v1; encontrou ${found.join(', ')}`,
    );
  }
  db.exec(`
    DROP TABLE players;
    DROP TABLE matches;
    DROP TABLE clubs;

    CREATE TABLE clubs (
      id         INTEGER PRIMARY KEY,
      name       TEXT NOT NULL,
      short_name TEXT NOT NULL,
      -- Mean overall of the squad when the world was made.
      strength   INTEGER NOT NULL,
      formation  INTEGER NOT NULL
    ) STRICT;

    CREATE TABLE players (
      id           INTEGER PRIMARY KEY,
      club_id      INTEGER REFERENCES clubs (id),
      name         TEXT NOT NULL,
      position     INTEGER NOT NULL,
      birth_year   INTEGER NOT NULL,
      overall      INTEGER NOT NULL,
      potential    INTEGER NOT NULL,
      -- Basis points, 0..10000.
      condition    INTEGER NOT NULL,
      morale       INTEGER NOT NULL,
      injury_weeks INTEGER NOT NULL,
      -- The whole static sheet (60 bytes) and the dynamic state (14 bytes),
      -- in the layouts of the fm-persistence crate.
      sheet        BLOB NOT NULL,
      dynamic      BLOB NOT NULL
    ) STRICT;
    CREATE INDEX players_by_club ON players (club_id);

    CREATE TABLE matches (
      id             INTEGER PRIMARY KEY,
      competition_id INTEGER NOT NULL REFERENCES competitions (id),
      round          INTEGER NOT NULL,
      day            INTEGER NOT NULL,
      home_club_id   INTEGER NOT NULL REFERENCES clubs (id),
      away_club_id   INTEGER NOT NULL REFERENCES clubs (id),
      -- 64 bits, unsigned, little-endian: SQLite's INTEGER is signed and
      -- JavaScript only keeps 53 bits exactly. (INTEGER in v1.)
      seed           BLOB NOT NULL,
      played         INTEGER NOT NULL DEFAULT 0,
      home_goals     INTEGER,
      away_goals     INTEGER,
      home_shots     INTEGER,
      away_shots     INTEGER,
      home_on_target INTEGER,
      away_on_target INTEGER
    ) STRICT;
    CREATE INDEX matches_by_round ON matches (competition_id, round);
    CREATE INDEX matches_by_day ON matches (day);
  `);
}

/** The game's own migrations of a save, in order (the list above, grown). */
export const SAVE_SCHEMA: readonly Migration[] = [
  ...SAVE_MIGRATIONS,
  { version: 2, name: 'world-v2', up: worldV2 },
];

/** Schema version the game writes. */
export const SAVE_SCHEMA_VERSION = SAVE_SCHEMA.length;

/**
 * Test migrations (`DbOptions.test.migrations`), numbered right after the
 * game's own: the chain is exercised one version ahead of the game.
 *  - `next`: adds a table.
 *  - `broken`: fails after changing the file — it must roll back whole.
 */
export function testMigrations(): { readonly next: Migration; readonly broken: Migration } {
  return {
    next: {
      version: SAVE_SCHEMA_VERSION + 1,
      name: 'test-next',
      up: (db) => db.exec('CREATE TABLE test_next (id INTEGER PRIMARY KEY, note TEXT) STRICT;'),
    },
    broken: {
      version: SAVE_SCHEMA_VERSION + 2,
      name: 'test-broken',
      up: (db) => {
        db.exec('CREATE TABLE test_broken (id INTEGER PRIMARY KEY) STRICT;');
        db.exec('INSERT INTO table_that_does_not_exist VALUES (1);');
      },
    },
  };
}
