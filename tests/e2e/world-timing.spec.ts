import { expect, test } from '@playwright/test';

// Fase 7B (7B.4): how long the world takes to live a round — the number
// that decides whether the matches of a day need a pool of workers (SPEC,
// 7B.4: under 500 ms the pool waits; over 1 s it is mandatory; the median
// of the whole round in the CI's Chromium decides).
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
type TimingHooks = {
  fmSave: { startDatabase(options?: unknown): Promise<{ client: { request(op: string, args: unknown): Promise<unknown> } }> };
  fmWorld: { startWorld(db: unknown): Promise<{ request(op: string, args: unknown): Promise<unknown> }> };
};

const median = (values: number[]): number => {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1 ? (sorted[mid] ?? 0) : ((sorted[mid - 1] ?? 0) + (sorted[mid] ?? 0)) / 2;
};
const mean = (values: number[]): number => values.reduce((sum, v) => sum + v, 0) / values.length;

/** Lives `days` days of a fresh world in one request; the page only waits. */
async function live(page: import('@playwright/test').Page, days: number) {
  await page.goto('/?view=blank');
  await expect(page.getByTestId('blank')).toBeVisible();
  return page.evaluate(async (count) => {
    const hooks = globalThis as unknown as TimingHooks;
    // The storage the browser has: OPFS, or IndexedDB where there is none.
    const db = await hooks.fmSave.startDatabase({});
    const storage = (await db.client.request('storage.info', {})) as { backend: string };
    const save = (await db.client.request('save.create', { name: 'Medição' })) as { id: string };
    await db.client.request('save.open', { id: save.id });
    const world = await hooks.fmWorld.startWorld(db);
    await world.request('world.new', { seed: '2026', userClub: 0 });
    const before = performance.now();
    const lived = (await world.request('world.advance', { days: count })) as Advance;
    return {
      lived,
      wallMs: performance.now() - before,
      cores: navigator.hardwareConcurrency,
      backend: storage.backend,
    };
  }, days);
}

test.describe('Fase 7B: world timing (alone, one worker)', () => {
  test.skip(process.env['FM_WORLD_TIMING'] === undefined, 'Timed alone, in its own CI step (FM_WORLD_TIMING=1)');

  test('five rounds: the median of the whole round decides the pool', async ({ page }, testInfo) => {
    test.setTimeout(300_000);
    // Five weeks: thirty days off and five rounds.
    const { lived, cores, backend } = await live(page, 35);
    const rounds = lived.timing;
    expect(lived.summary.day).toBe(35);
    expect(rounds.map((r) => [r.round, r.matches])).toEqual([0, 1, 2, 3, 4].map((r) => [r, 10]));

    const whole = median(rounds.map((r) => r.totalMs));
    const perMatch = median(rounds.map((r) => r.simulateMs / r.matches));
    const commit = mean(rounds.map((r) => r.commitMs));
    const overhead = median(rounds.map((r) => r.totalMs - r.simulateMs - r.commitMs));
    const name = testInfo.project.name;
    console.log(
      `[7B timing ${name}] round (whole, median of 5) ${whole.toFixed(0)} ms | match (median) ${perMatch.toFixed(0)} ms | commitDay (mean) ${commit.toFixed(1)} ms | rest of the round (median) ${overhead.toFixed(1)} ms | cores ${cores} | storage ${backend}`,
    );
    console.log(
      `[7B timing ${name}] rounds: ${rounds.map((r) => `${r.totalMs.toFixed(0)}`).join(', ')} ms | season estimate (38 × median) ${((38 * whole) / 1000).toFixed(0)} s`,
    );
    testInfo.annotations.push({
      type: 'world-timing',
      description: `round ${whole.toFixed(0)} ms, match ${perMatch.toFixed(0)} ms, commit ${commit.toFixed(1)} ms, ${cores} cores`,
    });
    // No limit here: what to do with the number is a decision (SPEC, 7B.4).
    // Only that the measurement itself makes sense.
    expect(whole).toBeGreaterThan(0);
    expect(rounds.every((r) => r.totalMs >= r.simulateMs)).toBe(true);
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
