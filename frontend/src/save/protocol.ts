// Protocol of the database worker (spec Fase 7A). The database worker is
// the only place that opens the save files and speaks SQL; everybody else —
// the page, and later the world and engine workers through a MessagePort —
// asks for domain operations with these messages.
//
//   request:  { id, op, args }
//   response: { id, ok: true, result } | { id, ok: false, error }
//
// Large payloads travel as transferred ArrayBuffers, never copied.

/** Where the save files live. */
export type StorageBackend =
  /** SQLite files in the origin private file system (`opfs-sahpool`). */
  | 'opfs'
  /** Fallback: databases in memory, each file stored whole in IndexedDB. */
  | 'idb'
  /** No usable storage: every other operation fails. */
  | 'none';

export type StorageInfo = {
  readonly backend: StorageBackend;
  /** SQLite version of the WASM build. */
  readonly sqlite: string;
  /** Why this is not `'opfs'`, when it is not. */
  readonly detail?: string;
  /** What checking the file opened last found and cost (IndexedDB only). */
  readonly lastOpen?: OpenCheck;
};

/**
 * The checks a file of the IndexedDB fallback goes through when it is
 * opened (spec Fase 7B, 7B.5), for the last one that passed them.
 */
export type OpenCheck = {
  readonly file: string;
  readonly bytes: number;
  /** The file had a sum stored beside it (files older than 7B.5 have none). */
  readonly summed: boolean;
  /** Computing the SHA-256 of the bytes read back (ms; 0 without a sum). */
  readonly sumMs: number;
  /** `PRAGMA integrity_check` (ms). */
  readonly integrityMs: number;
};

/** One row of the catalog: a save and the file that holds it. */
export type SaveInfo = {
  readonly id: string;
  readonly name: string;
  /** ISO timestamps. */
  readonly createdAt: string;
  readonly lastOpenedAt: string;
  readonly clubName: string;
  readonly season: number;
  readonly day: number;
  /** Name of the file in the pool that holds this save right now. */
  readonly activeFile: string;
  readonly schemaVersion: number;
};

/** What `save.open` reports about the file it opened. */
export type OpenedSave = {
  readonly save: SaveInfo;
  /** Schema versions the file went through in this open (empty: none). */
  readonly migrated: readonly number[];
  /** Tables of the file, in name order. */
  readonly tables: readonly string[];
  /** Names of the migrations recorded in the file, in version order. */
  readonly migrations: readonly string[];
};

export type MetaValue = string | number | null;

// The world inside a save (Fase 7B). The blobs are laid out by the Rust
// side (`fm-persistence`): sheets 60 bytes a player, dynamic states 14,
// line-ups 44 a club, match seeds 8 a match (u64, little-endian).

export type MatchResult = {
  readonly homeGoals: number;
  readonly awayGoals: number;
  readonly homeShots: number;
  readonly awayShots: number;
  readonly homeOnTarget: number;
  readonly awayOnTarget: number;
};

/** A club; its id is its place in the list. Tactics as level codes. */
export type WorldClub = {
  readonly name: string;
  readonly shortName: string;
  readonly strength: number;
  readonly formation: number;
  readonly mentality: number;
  readonly pressing: number;
  readonly width: number;
  readonly lineHeight: number;
};

/** What the screens query about a player; its id is its place in the list. */
export type WorldPlayer = {
  readonly club: number | null;
  readonly name: string;
  readonly position: number;
  readonly birthYear: number;
  readonly overall: number;
  readonly potential: number;
  /** Basis points, 0..10000. */
  readonly condition: number;
  readonly morale: number;
  readonly injuryWeeks: number;
};

/** A match of the season; its id is its place in the list. */
export type WorldFixture = {
  readonly round: number;
  readonly day: number;
  readonly home: number;
  readonly away: number;
  readonly result: MatchResult | null;
};

export type WorldSave = {
  /** The world seed: a u64 in decimal (JavaScript numbers keep 53 bits). */
  readonly seed: string;
  readonly day: number;
  readonly seasonYear: number;
  readonly userClub: number;
  readonly clubs: readonly WorldClub[];
  /** The starting elevens, one after the other, in club order. */
  readonly lineups: ArrayBuffer;
  readonly players: readonly WorldPlayer[];
  /** The static sheets and the dynamic states, in player order. */
  readonly sheets: ArrayBuffer;
  readonly dynamics: ArrayBuffer;
  readonly fixtures: readonly WorldFixture[];
  /** The match seeds, in match order. */
  readonly matchSeeds: ArrayBuffer;
};

/** The end of a day: what `world.commitDay` writes in one transaction. */
export type DayCommit = {
  /** The day the world is on once this is written: the stored day + 1. */
  readonly day: number;
  /** The matches of the day that ended, all of them. */
  readonly results: readonly (MatchResult & { readonly id: number })[];
  /** Every player's dynamic state, in player order. */
  readonly dynamics: ArrayBuffer;
  /** Every player's display columns, in player order. */
  readonly players: readonly {
    readonly condition: number;
    readonly morale: number;
    readonly injuryWeeks: number;
  }[];
};

export type StandingRow = {
  readonly club: number;
  readonly name: string;
  readonly shortName: string;
  readonly played: number;
  readonly won: number;
  readonly drawn: number;
  readonly lost: number;
  readonly goalsFor: number;
  readonly goalsAgainst: number;
  readonly points: number;
};

export type RoundMatch = {
  readonly id: number;
  readonly day: number;
  readonly home: number;
  readonly away: number;
  readonly result: MatchResult | null;
};

/** Operations: `args` → `result`. */
export type DbOps = {
  'storage.info': { args: Record<string, never>; result: StorageInfo };
  'save.list': { args: Record<string, never>; result: readonly SaveInfo[] };
  'save.create': { args: { readonly name: string }; result: SaveInfo };
  /** Opens the save (migrating its file if needed) and makes it current. */
  'save.open': { args: { readonly id: string }; result: OpenedSave };
  /** Closes the current save, if any. */
  'save.close': { args: Record<string, never>; result: null };
  'save.delete': { args: { readonly id: string }; result: null };
  /** Reads a key of the current save's `meta` table (`null` when absent). */
  'meta.get': { args: { readonly key: string }; result: MetaValue };
  /** Writes a key of the current save's `meta` table, in one transaction. */
  'meta.set': { args: { readonly key: string; readonly value: MetaValue }; result: null };
  /**
   * The save as a SQLite file: what the "export" button downloads. `bytes`
   * is transferred.
   */
  'save.export': {
    args: { readonly id: string };
    result: { readonly fileName: string; readonly bytes: ArrayBuffer };
  };
  /**
   * Replaces the save `id` with the file in `bytes` (transfer it). The file
   * is written aside, validated (a SQLite file, ours, sound, not from a
   * newer game) and migrated if older; only then the catalog is pointed at
   * it, in one transaction, and the old file removed. Any failure leaves
   * the save exactly as it was.
   */
  'save.import': { args: { readonly id: string; readonly bytes: ArrayBuffer }; result: SaveInfo };
  /**
   * SHA-256 (hex) of every table of the current save, row by row: two
   * saves with the same content have the same digest, whatever their files
   * look like byte by byte. `applied_at` of the migrations is left out.
   */
  'save.digest': { args: Record<string, never>; result: string };
  /**
   * Writes a new world into the current save, in one transaction. Refused
   * (`world-exists`) when the save already has one. Transfer the buffers.
   */
  'world.create': { args: { readonly world: WorldSave }; result: null };
  /**
   * The world of the current save as last committed; its buffers are
   * transferred. A save without a world (every save of Fase 7A) answers
   * `no-world`.
   */
  'world.load': { args: Record<string, never>; result: WorldSave };
  /**
   * Ends a day in one transaction. Only the day right after the stored one
   * is accepted (`out-of-order` otherwise): a commit sent twice changes
   * nothing.
   */
  'world.commitDay': { args: { readonly commit: DayCommit }; result: null };
  /** The league table, worked out from the results. */
  'world.standings': { args: Record<string, never>; result: readonly StandingRow[] };
  /** The matches of a round (0-based). */
  'world.round': { args: { readonly round: number }; result: readonly RoundMatch[] };
  /**
   * Test only (`DbOptions.test`): writes a `meta` key inside a transaction
   * that is never committed, and answers. Killing the worker afterwards is
   * a crash in the middle of a write.
   */
  'test.writeWithoutCommit': {
    args: { readonly key: string; readonly value: MetaValue };
    result: null;
  };
  /** Test only: the names of the files in the pool, sorted. */
  'test.files': { args: Record<string, never>; result: readonly string[] };
  /**
   * Test only: writes a file into the storage — `bytes` when given, else
   * one page with a SQLite header. Nobody references it (an orphan) unless
   * the test points the catalog at it.
   */
  'test.plantFile': { args: { readonly name: string; readonly bytes?: ArrayBuffer }; result: null };
  /**
   * Test only: runs SQL straight on a file of the storage, without opening
   * it as a save (so without migrating it), and returns the rows. `file`
   * `/catalog.sqlite` is the catalog.
   */
  'test.sql': {
    args: { readonly file: string; readonly sql: string };
    result: readonly (readonly (string | number | null)[])[];
  };
};

export type DbOp = keyof DbOps;

export type DbRequest<O extends DbOp = DbOp> = {
  readonly id: number;
  readonly op: O;
  readonly args: DbOps[O]['args'];
};

export type DbResponse<O extends DbOp = DbOp> =
  | { readonly id: number; readonly ok: true; readonly result: DbOps[O]['result'] }
  | { readonly id: number; readonly ok: false; readonly error: DbError };

/** Errors a caller can tell apart; `message` is for people. */
export type DbError = {
  readonly code:
    | 'no-storage'
    | 'not-found'
    | 'no-save-open'
    | 'newer-version'
    | 'not-a-save'
    | 'migration-failed'
    | 'no-world'
    | 'world-exists'
    | 'out-of-order'
    | 'invalid'
    /**
     * A file of the storage failed a check when it was opened: the message
     * names the file and says which check (the sum, or SQLite's own).
     */
    | 'corrupt'
    | 'internal';
  readonly message: string;
};

/** Start-up options of the worker (first message it receives). */
export type DbOptions = {
  /** `'idb'`: skip OPFS and use the IndexedDB fallback (tests, diagnosis). */
  readonly storage?: 'idb';
  /**
   * Test mode: enables the `test.*` operations and, with `migrations`,
   * appends test migrations to the save schema so that the chain can be
   * exercised before the game has a second schema version.
   *  - `'next'`: one more migration than the game has (adds a table).
   *  - `'next-then-broken'`: that one and another that fails half way.
   */
  readonly test?: {
    readonly migrations?: 'next' | 'next-then-broken';
    /**
     * `save.import` stops for good (never answers) right before or right
     * after the catalog is pointed at the new file: killing the worker then
     * is a crash at that exact point.
     */
    readonly stopImport?: 'before-swap' | 'after-swap';
    /**
     * `world.commitDay` writes the day and stops for good right before the
     * commit of its transaction: killing the worker then is a crash in the
     * middle of ending a day.
     */
    readonly stopCommitDay?: boolean;
  };
};

/** Control messages of the worker, besides requests. */
export type DbControl =
  | { readonly type: 'init'; readonly options: DbOptions }
  /** Hands the worker a port: requests arriving on it are served like the page's. */
  | { readonly type: 'connect'; readonly port: MessagePort };

export type DbReady = { readonly type: 'ready' };
