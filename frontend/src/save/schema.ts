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

/**
 * Test migrations (`DbOptions.test.migrations`): the chain has to work
 * before the game has a second schema version of its own.
 */
export const TEST_MIGRATION_V2: Migration = {
  version: 2,
  name: 'test-v2',
  up: (db) => db.exec('CREATE TABLE test_v2 (id INTEGER PRIMARY KEY, note TEXT) STRICT;'),
};

/** Fails after changing the file: the whole migration must roll back. */
export const TEST_MIGRATION_V3_BROKEN: Migration = {
  version: 3,
  name: 'test-v3-broken',
  up: (db) => {
    db.exec('CREATE TABLE test_v3 (id INTEGER PRIMARY KEY) STRICT;');
    db.exec('INSERT INTO table_that_does_not_exist VALUES (1);');
  },
};
