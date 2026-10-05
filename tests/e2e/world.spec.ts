import { expect, test, type Page } from '@playwright/test';
import { ask, open, setBackend, type Hooks, type SaveInfo } from './support/db';

// Fase 7B (7B.4): the world worker. The real world (the Rust `WorldHost`,
// in WASM) lives in a worker of its own, simulates the days in LOD Abstract
// and writes them through the database worker, over a port. Three
// processes: the page, the world worker, the database worker.

type Summary = {
  seed: string;
  day: number;
  seasonYear: number;
  userClub: number;
  nextRound: number;
  matchesToday: number;
  finished: boolean;
};
type Timing = { day: number; round: number; matches: number; simulateMs: number; commitMs: number; totalMs: number };
type Advance = { summary: Summary; daysLived: number; timing: Timing[] };
type WorldHandle = { request(op: string, args: unknown): Promise<unknown>; terminate(): void };
type WorldHooks = Hooks & {
  fmWorld: { startWorld(db: Hooks['db']): Promise<WorldHandle> };
  /** The world worker the test is talking to. */
  world: WorldHandle;
};

/** Starts (or restarts, after killing it) the world worker over the page's database. */
async function startWorld(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const hooks = globalThis as unknown as Partial<WorldHooks> & Pick<WorldHooks, 'fmWorld' | 'db'>;
    hooks.world?.terminate();
    hooks.world = await hooks.fmWorld.startWorld(hooks.db);
  });
}

/** One request to the world worker; a failed one comes back as `{ error, message }`. */
async function world<T>(page: Page, op: string, args: unknown = {}): Promise<T> {
  return page.evaluate(
    async ([o, a]) => {
      const hooks = globalThis as unknown as WorldHooks;
      try {
        return (await hooks.world.request(o as string, a)) as never;
      } catch (err: unknown) {
        const e = err as { code?: string; message?: string };
        return { error: e.code ?? String(err), message: e.message ?? '' } as never;
      }
    },
    [op, args] as const,
  );
}

/** A fresh save, opened in the database worker. */
async function openSave(page: Page, name: string): Promise<SaveInfo> {
  const save = await ask<SaveInfo>(page, 'save.create', { name });
  await ask(page, 'save.open', { id: save.id });
  return save;
}

type Loaded = {
  seed: string;
  day: number;
  userClub: number;
  clubs: { name: string; shortName: string; strength: number; formation: number }[];
  players: { club: number | null; name: string; overall: number; condition: number }[];
  fixtures: { round: number; day: number; home: number; away: number; result: { homeGoals: number; awayGoals: number } | null }[];
};
type Row = { club: number; played: number; won: number; drawn: number; lost: number; goalsFor: number; goalsAgainst: number; points: number };

/**
 * The Firefox of the test runner runs this WASM about 7× slower than
 * Chromium: a round of ten matches takes ~8 s there against ~1.1 s (the
 * owner's machine, 2026-10-05; cause not investigated — SPEC, 7B.4). The
 * tests that live several rounds get a longer limit in Firefox only; the
 * limit everywhere else stays the default, so a slowdown still shows.
 */
const FIREFOX_TIMEOUT_MS = 180_000;

for (const backend of ['opfs', 'idb'] as const) {
  test.describe(`Fase 7B: the world worker (${backend})`, () => {
    test.beforeEach(({ browserName }) => {
      setBackend(backend);
      if (browserName === 'firefox') test.setTimeout(FIREFOX_TIMEOUT_MS);
    });

    test('world.new makes the world and writes it; world.advance lives the days into the database', async ({
      page,
    }, testInfo) => {
      await open(page, testInfo.project.name);
      const save = await openSave(page, 'Mundo de verdade');
      await startWorld(page);
      // Nothing loaded yet.
      expect(await world(page, 'world.advance', { days: 1 })).toMatchObject({ error: 'no-world-loaded' });

      const made = await world<Summary>(page, 'world.new', { seed: '2026', userClub: 7 });
      expect(made).toEqual({
        seed: '2026',
        day: 0,
        seasonYear: 2026,
        userClub: 7,
        nextRound: 0,
        matchesToday: 0,
        finished: false,
      });
      // The page reads the world straight from the database: a league.
      const stored = await ask<Loaded>(page, 'world.load');
      expect([stored.seed, stored.day, stored.userClub]).toEqual(['2026', 0, 7]);
      expect(stored.clubs).toHaveLength(20);
      expect(stored.players).toHaveLength(500);
      expect(stored.fixtures).toHaveLength(380);
      expect(stored.clubs[0]).toMatchObject({ name: 'Atlético Aurora', shortName: 'AUR', formation: 0 });
      expect(stored.players.every((p) => /^\S+ \S+$/.test(p.name) && p.overall >= 1 && p.overall <= 100)).toBe(true);
      expect(stored.players.filter((p) => p.club === 7)).toHaveLength(25);
      expect(stored.fixtures.every((f) => f.result === null)).toBe(true);
      expect(await ask<SaveInfo[]>(page, 'save.list')).toMatchObject([{ id: save.id, clubName: 'Harmonia Futebol Clube', day: 0 }]);

      // A second world in the same save is refused, as the database says.
      expect(await world(page, 'world.new', { seed: '5', userClub: 0 })).toMatchObject({ error: 'world-exists' });
      expect(await world(page, 'world.new', { seed: 'x', userClub: 0 })).toMatchObject({ error: 'invalid' });
      expect(await world(page, 'world.advance', { days: 0 })).toMatchObject({ error: 'invalid' });

      // Six days off, then the first round: a week.
      const week = await world<Advance>(page, 'world.advance', { days: 7 });
      expect(week.daysLived).toBe(7);
      expect(week.summary).toMatchObject({ day: 7, nextRound: 1, matchesToday: 0, finished: false });
      expect(week.timing).toHaveLength(1);
      expect(week.timing[0]).toMatchObject({ day: 6, round: 0, matches: 10 });
      const { simulateMs, commitMs, totalMs } = week.timing[0]!;
      expect(simulateMs).toBeGreaterThan(0);
      expect(totalMs).toBeGreaterThanOrEqual(simulateMs + commitMs - 1);
      console.log(
        `[7B round ${testInfo.project.name} ${backend}] simulate ${simulateMs.toFixed(0)} ms, commit ${commitMs.toFixed(0)} ms, whole round ${totalMs.toFixed(0)} ms (tests in parallel: not the number that decides the pool)`,
      );

      // The database has the round, and its table adds up.
      const after = await ask<Loaded>(page, 'world.load');
      expect(after.day).toBe(7);
      expect(after.fixtures.filter((f) => f.result !== null).map((f) => f.round)).toEqual(new Array(10).fill(0));
      expect(after.players.some((p) => p.condition < 10_000)).toBe(true);
      const table = await ask<Row[]>(page, 'world.standings');
      expect(table.every((row) => row.played === 1)).toBe(true);
      const goals = after.fixtures.reduce((sum, f) => sum + (f.result === null ? 0 : f.result.homeGoals + f.result.awayGoals), 0);
      expect(table.reduce((sum, row) => sum + row.goalsFor, 0)).toBe(goals);
      expect(table.reduce((sum, row) => sum + row.goalsAgainst, 0)).toBe(goals);
      expect(await ask<SaveInfo[]>(page, 'save.list')).toMatchObject([{ id: save.id, day: 7 }]);
    });

    test('world.open loads the world back; a save without a world says no-world', async ({ page }, testInfo) => {
      await open(page, testInfo.project.name);
      const empty = await openSave(page, 'Sem mundo');
      await startWorld(page);
      const refused = await world<{ error: string; message: string }>(page, 'world.open');
      expect(refused).toEqual({ error: 'no-world', message: 'este save não tem mundo; foi criado antes da Fase 7B' });

      // A world, a week; then the world worker is killed and started again.
      await world(page, 'world.new', { seed: '11', userClub: 2 });
      await world(page, 'world.advance', { days: 7 });
      const digest = await ask<string>(page, 'save.digest');
      await startWorld(page);
      expect(await world(page, 'world.advance', { days: 1 })).toMatchObject({ error: 'no-world-loaded' });
      const back = await world<Summary>(page, 'world.open');
      expect(back).toMatchObject({ seed: '11', day: 7, userClub: 2, nextRound: 1 });
      expect(await ask<string>(page, 'save.digest')).toBe(digest); // opening writes nothing

      // It goes on from there, to the same place as a world that never
      // stopped: another save, same seed, fourteen days straight.
      await world(page, 'world.advance', { days: 7 });
      const resumed = await ask<string>(page, 'save.digest');
      const straight = await openSave(page, 'De uma vez');
      await world(page, 'world.new', { seed: '11', userClub: 2 });
      expect((await world<Advance>(page, 'world.advance', { days: 14 })).summary.day).toBe(14);
      // The two saves differ only by their names (in `meta`).
      await ask(page, 'meta.set', { key: 'name', value: 'Sem mundo' });
      expect(await ask<string>(page, 'save.digest')).toBe(resumed);
      expect(straight.id).not.toBe(empty.id);
    });
  });
}
