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
