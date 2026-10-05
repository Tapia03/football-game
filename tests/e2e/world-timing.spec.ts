import { expect, test } from '@playwright/test';

// Fase 7B (7B.4, 7B.4b): how long the world takes to live a round. Without
// a pool it was the number that decided that the matches of a day need a
// pool of match workers (SPEC, 7B.4: over 1 s in the CI's Chromium, the
// pool is mandatory); with the pool, beside it, it is what the pool gives
// (SPEC, 7B.4b: 700 ms or less in the CI's Chromium).
//
// Timing is only worth anything alone: other tests running beside it
// inflate it. So this file is skipped in the normal run and has a step of
// its own in the CI, with a single Playwright worker:
//
//   FM_WORLD_TIMING=1 npx playwright test tests/e2e/world-timing.spec.ts --workers=1
//
// `FM_WORLD_SEASON=1` adds the whole season (minutes; the CI sets it only
// when the workflow is dispatched by hand).

type Timing = {
  day: number;
  round: number;
  matches: number;
  simulateMs: number;
  matchMs: number;
  players: number;
  commitMs: number;
  totalMs: number;
};
type Advance = { summary: { day: number; finished: boolean }; daysLived: number; timing: Timing[] };
type Stats = { players: number; dropped: string[]; wasmBytes: { world: number; players: number[] } };
type TimingHooks = {
  fmSave: { startDatabase(options?: unknown): Promise<{ client: { request(op: string, args: unknown): Promise<unknown> } }> };
  fmWorld: {
    startWorld(
      db: unknown,
      options?: { players?: number },
    ): Promise<{ players: number; request(op: string, args: unknown): Promise<unknown> }>;
  };
};

const median = (values: number[]): number => {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1 ? (sorted[mid] ?? 0) : ((sorted[mid - 1] ?? 0) + (sorted[mid] ?? 0)) / 2;
};
const mean = (values: number[]): number => values.reduce((sum, v) => sum + v, 0) / values.length;
const megabytes = (bytes: number): string => (bytes / (1024 * 1024)).toFixed(1);

/**
 * Lives `days` days of a fresh world in one request; the page only waits.
 * `players`: the size of the pool of match workers (0: none; not given: the
 * one the game picks for the machine).
 */
async function live(page: import('@playwright/test').Page, days: number, players?: number) {
  // A page of its own for each measurement: the workers of the one before
  // are gone with it.
  await page.goto('/?view=blank');
  await expect(page.getByTestId('blank')).toBeVisible();
  return page.evaluate(
    async ([count, size]) => {
      const hooks = globalThis as unknown as TimingHooks;
      // The storage the browser has: OPFS, or IndexedDB where there is none.
      const db = await hooks.fmSave.startDatabase({});
      const storage = (await db.client.request('storage.info', {})) as { backend: string };
      const save = (await db.client.request('save.create', { name: 'Medição' })) as { id: string };
      await db.client.request('save.open', { id: save.id });
      const world = await hooks.fmWorld.startWorld(db, size === null ? {} : { players: size });
      await world.request('world.new', { seed: '2026', userClub: 0 });
      const before = performance.now();
      const lived = (await world.request('world.advance', { days: count })) as Advance;
      const wallMs = performance.now() - before;
      const stats = (await world.request('world.stats', {})) as Stats;
      const digest = (await db.client.request('save.digest', {})) as string;
      return {
        lived,
        wallMs,
        stats,
        digest,
        started: world.players,
        cores: navigator.hardwareConcurrency,
        backend: storage.backend,
      };
    },
    [days, players ?? null] as const,
  );
}

test.describe('Fase 7B: world timing (alone, one worker)', () => {
  test.skip(process.env['FM_WORLD_TIMING'] === undefined, 'Timed alone, in its own CI step (FM_WORLD_TIMING=1)');

  test('six rounds, with and without the pool: the median of the whole round', async ({ page }, testInfo) => {
    test.setTimeout(600_000);
    const name = testInfo.project.name;
    // Six weeks: six rounds. The first is reported apart — with a pool it
    // carries the loading of the world into every match worker — and the
    // median is of the other five, with and without the pool alike.
    const DAYS = 42;
    const cores = await page.evaluate(() => navigator.hardwareConcurrency || 1);
    // Without a pool; with one match worker less than there are cores (the
    // world worker and the database keep one); and the pool the game picks.
    const sizes = [...new Set([0, Math.max(1, Math.min(cores, 10) - 1), Math.min(cores, 10)])];
    const medians = new Map<number, number>();
    const digests = new Set<string>();
    for (const size of sizes) {
      const { lived, stats, digest, started, backend } = await live(page, DAYS, size);
      expect(lived.summary.day).toBe(DAYS);
      expect(started).toBe(size);
      expect(stats.dropped).toEqual([]);
      expect(lived.timing.map((r) => [r.round, r.matches, r.players])).toEqual(
        [0, 1, 2, 3, 4, 5].map((r) => [r, 10, size]),
      );
      digests.add(digest);
      const [first, ...rounds] = lived.timing;
      const whole = median(rounds.map((r) => r.totalMs));
      const playing = median(rounds.map((r) => r.simulateMs));
      // A match as whoever played it timed it: it grows when the players
      // share cores.
      const perMatch = median(rounds.map((r) => r.matchMs / r.matches));
      const commit = mean(rounds.map((r) => r.commitMs));
      const memory = stats.wasmBytes.world + stats.wasmBytes.players.reduce((sum, b) => sum + b, 0);
      medians.set(size, whole);
      console.log(
        `[7B timing ${name}] pool ${size}: round (whole, median of 5) ${whole.toFixed(0)} ms | first round ${first!.totalMs.toFixed(0)} ms | matches of a round ${playing.toFixed(0)} ms | one match, by its player ${perMatch.toFixed(0)} ms | commitDay (mean) ${commit.toFixed(1)} ms | rounds ${rounds.map((r) => r.totalMs.toFixed(0)).join(', ')} | season estimate ${((38 * whole) / 1000).toFixed(0)} s`,
      );
      console.log(
        `[7B timing ${name}] pool ${size}: WASM memory ${megabytes(memory)} MB — world worker ${megabytes(stats.wasmBytes.world)} MB, match workers ${stats.wasmBytes.players.map(megabytes).join(' + ') || 'none'} MB | cores ${cores} | storage ${backend}`,
      );
      testInfo.annotations.push({
        type: 'world-timing',
        description: `pool ${size}: round ${whole.toFixed(0)} ms, first ${first!.totalMs.toFixed(0)} ms, match ${perMatch.toFixed(0)} ms, WASM ${megabytes(memory)} MB, ${cores} cores`,
      });
      expect(whole).toBeGreaterThan(0);
      expect(rounds.every((r) => r.totalMs >= r.simulateMs)).toBe(true);
    }
    const alone = medians.get(0) ?? 0;
    console.log(
      `[7B timing ${name}] round by pool size: ${[...medians].map(([size, ms]) => `${size}: ${ms.toFixed(0)} ms (${(alone / ms).toFixed(2)}×)`).join(' | ')}`,
    );
    // No limit on the time: what to do with the numbers is a decision
    // (SPEC, 7B.4b). The saves, though, must be one and the same.
    expect(digests.size).toBe(1);
  });

  test('the whole season, when asked for', async ({ page }, testInfo) => {
    test.skip(process.env['FM_WORLD_SEASON'] === undefined, 'Minutes long: only with FM_WORLD_SEASON=1');
    test.setTimeout(1_500_000);
    const { lived, wallMs, cores } = await live(page, 266);
    expect(lived.summary).toMatchObject({ day: 266, finished: true });
    expect(lived.timing).toHaveLength(38);
    const rounds = lived.timing.map((r) => r.totalMs);
    console.log(
      `[7B season ${testInfo.project.name}] 380 matches in ${(wallMs / 1000).toFixed(1)} s | round median ${median(rounds).toFixed(0)} ms, slowest ${Math.max(...rounds).toFixed(0)} ms | cores ${cores}`,
    );
  });
});
