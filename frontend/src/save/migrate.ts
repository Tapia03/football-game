// Schema migration on open (spec Fase 7A): the migrations a file has not
// run yet are applied in a chain, each in its own transaction together
// with its row in `migrations` and the new `PRAGMA user_version`. A file
// that dies half way is back at the previous version, whole.

import { APPLICATION_ID, type Migration } from './schema';

/** The slice of the SQLite API the chain needs (see `db.worker.ts`). */
export type MigratableDb = {
  exec(sql: string): void;
  run(sql: string, bind: readonly (string | number | null)[]): void;
  value(sql: string): number;
};

export class NewerVersionError extends Error {
  constructor(
    readonly fileVersion: number,
    readonly knownVersion: number,
  ) {
    super(`o arquivo é da versão ${fileVersion} do schema; este jogo conhece até a ${knownVersion}`);
  }
}

export class NotOursError extends Error {
  constructor(readonly applicationId: number) {
    super(`o arquivo não é um save deste jogo (application_id ${applicationId})`);
  }
}

export class MigrationFailedError extends Error {
  constructor(
    readonly version: number,
    readonly migrationName: string,
    cause: unknown,
  ) {
    super(
      `a migração ${version} (${migrationName}) falhou: ${cause instanceof Error ? cause.message : String(cause)}`,
    );
  }
}

/** Schema version the file is at. */
export function schemaVersion(db: MigratableDb): number {
  return db.value('PRAGMA user_version');
}

/**
 * Checks that `db` is ours and not from a newer game, without changing it.
 * An empty file (version 0, no application id) is a file about to be born.
 */
export function checkFile(db: MigratableDb, migrations: readonly Migration[]): void {
  const version = schemaVersion(db);
  const applicationId = db.value('PRAGMA application_id');
  if (version > 0 && applicationId !== APPLICATION_ID) throw new NotOursError(applicationId);
  const known = migrations.at(-1)?.version ?? 0;
  if (version > known) throw new NewerVersionError(version, known);
}

/**
 * Brings `db` to the last version of `migrations`. Returns the versions
 * applied, in order (empty when the file was already current).
 *
 * Throws `NewerVersionError` (nothing touched) when the file is from a
 * newer game, and `MigrationFailedError` when a migration fails — the file
 * then stays at the last version that did apply.
 */
export function migrate(db: MigratableDb, migrations: readonly Migration[]): number[] {
  checkFile(db, migrations);
  const applied: number[] = [];
  for (const migration of migrations) {
    if (migration.version <= schemaVersion(db)) continue;
    db.exec('BEGIN IMMEDIATE');
    try {
      migration.up(db);
      db.run('INSERT INTO migrations (version, name, applied_at) VALUES (?, ?, ?)', [
        migration.version,
        migration.name,
        new Date().toISOString(),
      ]);
      // PRAGMAs take no bound parameters; both values are our own integers.
      db.exec(`PRAGMA application_id = ${APPLICATION_ID}`);
      db.exec(`PRAGMA user_version = ${migration.version}`);
      db.exec('COMMIT');
    } catch (err: unknown) {
      db.exec('ROLLBACK');
      throw new MigrationFailedError(migration.version, migration.name, err);
    }
    applied.push(migration.version);
  }
  return applied;
}
