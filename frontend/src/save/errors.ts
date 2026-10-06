import type { DbError } from './protocol';

/** A failed operation of the database worker, with a code the caller can act on. */
export class OpError extends Error {
  constructor(
    readonly code: DbError['code'],
    message: string,
  ) {
    super(message);
  }
}

/**
 * A file of the storage that is not what was stored, or not a sound
 * database (spec Fase 7B, 7B.5). `check` says which: `'sum'` — the bytes
 * read back are not the bytes written; `'integrity'` — the bytes are the
 * ones written (or have no sum to say), and SQLite finds them malformed.
 */
export class FileCorruptError extends Error {
  constructor(
    readonly file: string,
    readonly check: 'sum' | 'integrity',
    detail: string,
  ) {
    super(`arquivo ${file}: ${detail}`);
  }
}
