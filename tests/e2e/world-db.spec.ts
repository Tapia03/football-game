import { readFile } from 'node:fs/promises';
import { expect, test, type Page } from '@playwright/test';
import {
  ask,
  digestOf,
  exportSave,
  importSave,
  open,
  setBackend,
  start,
  type Hooks,
  type SaveInfo,
} from './support/db';

// Fase 7B (7B.3): the world inside a save — `world.create`, `world.load`,
// `world.commitDay`, `world.standings`, `world.round` — over both kinds of
// storage. The worlds here are made by hand: the database does not read
// the blobs, it only stores them (the Rust side lays them out and is
// tested natively). The real world meets the database in the next commits.

const V1_FIXTURE = 'tests/fixtures/save-v1-7a.sqlite';
const CLUBS = 20;
const PLAYERS = 500;
const MATCHES = 380;
const SHEET = 60;
const DYNAMIC = 14;
const LINEUP = 44;
const SEED = 8;

type Result = {
  homeGoals: number;
  awayGoals: number;
  homeShots: number;
  awayShots: number;
  homeOnTarget: number;
  awayOnTarget: number;
};
/** A world with its buffers as plain numbers, so that it crosses to the page. */
type World = {
  seed: string;
  day: number;
  seasonYear: number;
  userClub: number;
  clubs: {
    name: string;
    shortName: string;
    strength: number;
    formation: number;
    mentality: number;
    pressing: number;
    width: number;
    lineHeight: number;
  }[];
  lineups: number[];
  players: {
    club: number | null;
    name: string;
    position: number;
    birthYear: number;
    overall: number;
    potential: number;
    condition: number;
    morale: number;
    injuryWeeks: number;
  }[];
  sheets: number[];
  dynamics: number[];
  fixtures: { round: number; day: number; home: number; away: number; result: Result | null }[];
  matchSeeds: number[];
};
type Commit = {
  day: number;
  results: (Result & { id: number })[];
  dynamics: number[];
  players: { condition: number; morale: number; injuryWeeks: number }[];
};

/** Bytes that depend on where they are, so that a swapped blob shows. */
const bytes = (count: number, salt: number): number[] =>
  Array.from({ length: count }, (_, i) => (i * 31 + salt * 7) % 251);

/** A league of the right shape: 20 clubs, 500 players, 38 rounds of 10. */
function makeWorld(): World {
  return {
    // Larger than 2^53: the seed must survive as text.
    seed: '18446744073709551557',
    day: 0,
    seasonYear: 2026,
    userClub: 3,
    clubs: Array.from({ length: CLUBS }, (_, c) => ({
      name: `Clube ${String.fromCharCode(65 + c)}`,
      shortName: `C${String(c).padStart(2, '0')}`,
      strength: 50 + c,
      formation: 0,
      mentality: 2,
      pressing: 1,
      width: 1,
      lineHeight: 1,
    })),
    lineups: bytes(CLUBS * LINEUP, 1),
    players: Array.from({ length: PLAYERS }, (_, p) => ({
      // One free agent, to carry a NULL club through.
      club: p === PLAYERS - 1 ? null : Math.floor(p / 25),
      name: `Jogador ${p}`,
      position: p % 12,
      birthYear: 1990 + (p % 15),
      overall: 40 + (p % 50),
      potential: 50 + (p % 50),
      condition: 10_000,
      morale: 5_000,
      injuryWeeks: 0,
    })),
    sheets: bytes(PLAYERS * SHEET, 2),
    dynamics: bytes(PLAYERS * DYNAMIC, 3),
    fixtures: Array.from({ length: MATCHES }, (_, m) => {
      const round = Math.floor(m / 10);
      const i = m % 10;
      // Ten disjoint pairs a round; which pairs does not matter here.
      const home = (2 * i + round) % CLUBS;
      const away = (2 * i + 1 + round) % CLUBS;
      return { round, day: 7 * round + 6, home, away, result: null };
    }),
    matchSeeds: bytes(MATCHES * SEED, 4),
  };
}

/** The end of the day `day - 1` of `world`: a result for each of its matches. */
function makeCommit(world: World, day: number, salt = 0): Commit {
  const results = world.fixtures
    .map((f, id) => ({ f, id }))
    .filter(({ f }) => f.day === day - 1)
    .map(({ id }) => ({
      id,
      homeGoals: (id + salt) % 4,
      awayGoals: (id * 3 + salt) % 3,
      homeShots: 10 + (id % 7),
      awayShots: 8 + (id % 5),
      homeOnTarget: 3 + (id % 4),
      awayOnTarget: 2 + (id % 3),
    }));
  return {
    day,
    results,
    dynamics: bytes(PLAYERS * DYNAMIC, 100 + day + salt),
    players: Array.from({ length: PLAYERS }, (_, p) => ({
      condition: 10_000 - ((p + day) % 2_000),
      morale: 5_000 + ((p * day) % 1_000),
      injuryWeeks: (p + day) % 97 === 0 ? 2 : 0,
    })),
  };
}

/** `world.create`; a failed one comes back as `{ error: code }`. */
async function createWorld(page: Page, world: World): Promise<unknown> {
  return page.evaluate(async (w) => {
    const { db } = globalThis as unknown as Hooks;
    const buffers = {
      lineups: new Uint8Array(w.lineups).buffer,
      sheets: new Uint8Array(w.sheets).buffer,
      dynamics: new Uint8Array(w.dynamics).buffer,
      matchSeeds: new Uint8Array(w.matchSeeds).buffer,
    };
    try {
      return await db.client.request('world.create', { world: { ...w, ...buffers } }, Object.values(buffers));
    } catch (err: unknown) {
      return { error: (err as { code?: string }).code ?? String(err) };
    }
  }, world);
}

/** `world.load`, with its buffers back as plain numbers. */
async function loadWorld(page: Page): Promise<World | { error: string }> {
  return page.evaluate(async () => {
    const { db } = globalThis as unknown as Hooks;
    try {
      const w = (await db.client.request('world.load', {})) as Record<string, unknown>;
      const plain = (buffer: unknown): number[] => Array.from(new Uint8Array(buffer as ArrayBuffer));
      return {
        ...w,
        lineups: plain(w['lineups']),
        sheets: plain(w['sheets']),
        dynamics: plain(w['dynamics']),
        matchSeeds: plain(w['matchSeeds']),
      } as never;
    } catch (err: unknown) {
      return { error: (err as { code?: string }).code ?? String(err) };
    }
  });
}

/**
 * `world.commitDay`. A failed one comes back as `{ error, message }`; with
 * `wait` false the answer is not awaited (the worker is about to stop).
 */
async function commitDay(page: Page, commit: Commit, wait = true): Promise<unknown> {
  return page.evaluate(
    async ([c, awaited]) => {
      const { db } = globalThis as unknown as Hooks;
      const given = c as Commit;
      const dynamics = new Uint8Array(given.dynamics).buffer;
      const asked = db.client
        .request('world.commitDay', { commit: { ...given, dynamics } }, [dynamics])
        .catch((err: unknown) => {
          const e = err as { code?: string; message?: string };
          return { error: e.code ?? String(err), message: e.message ?? '' };
        });
      if (awaited) return asked;
      await new Promise((resolve) => setTimeout(resolve, 1_000));
      return 'not awaited';
    },
    [commit, wait] as const,
  );
}

/** A fresh save, opened, with the hand-made world in it. */
async function saveWithWorld(page: Page, name: string): Promise<{ save: SaveInfo; world: World }> {
  const save = await ask<SaveInfo>(page, 'save.create', { name });
  await ask(page, 'save.open', { id: save.id });
  const world = makeWorld();
  expect(await createWorld(page, world)).toBeNull();
  return { save, world };
}

for (const backend of ['opfs', 'idb'] as const) {
  test.describe(`Fase 7B: the world in the database worker (${backend})`, () => {
    test.beforeEach(() => {
      setBackend(backend);
    });

    test('a save without a world says so (a real save of Fase 7A, and a new one) and can be given one', async ({
      page,
    }, testInfo) => {
      await open(page, testInfo.project.name);
      const v1 = Array.from(await readFile(V1_FIXTURE));
      const save = await ask<SaveInfo>(page, 'save.create', { name: 'Veio da 7A' });
      expect(((await importSave(page, save.id, v1)) as SaveInfo).schemaVersion).toBe(2);
      await ask(page, 'save.open', { id: save.id });
      // Refused with a clear answer, by every operation that needs a world.
      const refusal = await page.evaluate(async () => {
        const { db } = globalThis as unknown as Hooks;
        try {
          await db.client.request('world.load', {});
          return { code: 'loaded', message: '' };
        } catch (err: unknown) {
          const e = err as { code?: string; message?: string };
          return { code: e.code ?? '', message: e.message ?? '' };
        }
      });
      expect(refusal).toEqual({ code: 'no-world', message: 'este save não tem mundo; foi criado antes da Fase 7B' });
      expect(await ask(page, 'world.standings')).toEqual({ error: 'no-world' });
      expect(await ask(page, 'world.round', { round: 0 })).toEqual({ error: 'no-world' });
      expect(await commitDay(page, makeCommit(makeWorld(), 1))).toMatchObject({ error: 'no-world' });
      // Nothing broke: the save is still the save Fase 7A wrote.
      expect(await ask(page, 'meta.get', { key: 'name' })).toBe('Save da Fase 7A');
      expect(await ask(page, 'meta.get', { key: 'club' })).toBe('Atlético Sintético');

      // A save made today has no world either, until one is created in it.
      const fresh = await ask<SaveInfo>(page, 'save.create', { name: 'De hoje' });
      await ask(page, 'save.open', { id: fresh.id });
      expect(await loadWorld(page)).toEqual({ error: 'no-world' });
      const world = makeWorld();
      expect(await createWorld(page, world)).toBeNull();
      expect(await loadWorld(page)).toEqual(world);
      // …and so can the save that came from Fase 7A.
      await ask(page, 'save.open', { id: save.id });
      expect(await createWorld(page, world)).toBeNull();
      expect(await loadWorld(page)).toEqual(world);
      expect(await ask(page, 'meta.get', { key: 'name' })).toBe('Save da Fase 7A');
    });

    test('world.create then world.load: the same world, blobs byte for byte; once only', async ({
      page,
    }, testInfo) => {
      await open(page, testInfo.project.name);
      const { save, world } = await saveWithWorld(page, 'Mundo');
      expect(await loadWorld(page)).toEqual(world);
      // The catalog shows the club, the season and the day of the world.
      expect(await ask<SaveInfo[]>(page, 'save.list')).toMatchObject([
        { id: save.id, clubName: 'Clube D', season: 2026, day: 0 },
      ]);
      // A second world in the same save is refused, and changes nothing.
      const before = await ask<string>(page, 'save.digest');
      const other = { ...makeWorld(), userClub: 9 };
      expect(await createWorld(page, other)).toEqual({ error: 'world-exists' });
      expect(await ask<string>(page, 'save.digest')).toBe(before);

      // After a restart it is all there.
      await start(page);
      await ask(page, 'save.open', { id: save.id });
      expect(await loadWorld(page)).toEqual(world);
      expect(await ask<string>(page, 'save.digest')).toBe(before);

      // A world of the wrong shape is refused whole: nothing is written.
      const empty = await ask<SaveInfo>(page, 'save.create', { name: 'Torto' });
      await ask(page, 'save.open', { id: empty.id });
      const clean = await ask<string>(page, 'save.digest');
      for (const broken of [
        { ...makeWorld(), sheets: bytes(PLAYERS * SHEET - 1, 2) },
        { ...makeWorld(), dynamics: bytes(PLAYERS * DYNAMIC + 14, 3) },
        { ...makeWorld(), lineups: bytes(LINEUP, 1) },
        { ...makeWorld(), matchSeeds: bytes(MATCHES * SEED - 8, 4) },
        { ...makeWorld(), userClub: 20 },
        { ...makeWorld(), seed: '-1' },
      ]) {
        expect(await createWorld(page, broken)).toEqual({ error: 'invalid' });
        expect(await loadWorld(page)).toEqual({ error: 'no-world' });
        expect(await ask<string>(page, 'save.digest')).toBe(clean);
      }
    });

    test('world.commitDay: one day at a time, in order; the table adds up', async ({ page }, testInfo) => {
      await open(page, testInfo.project.name);
      const { save, world } = await saveWithWorld(page, 'Dias');
      // Days 0 to 5 have no match: each ends with no result.
      for (let day = 1; day <= 6; day += 1) {
        expect(await commitDay(page, makeCommit(world, day))).toBeNull();
      }
      const onDay6 = await ask<string>(page, 'save.digest');
      // The same day again, a day skipped, a day back: refused, nothing changes.
      for (const wrong of [6, 8, 3, 0]) {
        const refused = (await commitDay(page, makeCommit(world, wrong))) as { error: string; message: string };
        expect(refused.error).toBe('out-of-order');
        expect(refused.message).toContain('o save está no dia 6');
        expect(await ask<string>(page, 'save.digest')).toBe(onDay6);
      }
      // Day 6 is round 0: its ten results, all of them and only them.
      const round0 = makeCommit(world, 7);
      expect(round0.results.map((r) => r.id)).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
      for (const wrong of [
        { ...round0, results: round0.results.slice(1) },
        { ...round0, results: [] },
        { ...round0, results: [...round0.results.slice(1), { ...round0.results[0]!, id: 10 }] },
        { ...round0, dynamics: bytes(PLAYERS * DYNAMIC - 1, 9) },
        { ...round0, players: round0.players.slice(1) },
      ]) {
        expect(await commitDay(page, wrong)).toMatchObject({ error: 'invalid' });
        expect(await ask<string>(page, 'save.digest')).toBe(onDay6);
      }
      expect(await commitDay(page, round0)).toBeNull();

      // What was committed is what is loaded: results, states, the day.
      const loaded = (await loadWorld(page)) as World;
      expect(loaded.day).toBe(7);
      expect(loaded.dynamics).toEqual(round0.dynamics);
      expect(loaded.sheets).toEqual(world.sheets);
      expect(loaded.players.map((p) => [p.condition, p.morale, p.injuryWeeks])).toEqual(
        round0.players.map((p) => [p.condition, p.morale, p.injuryWeeks]),
      );
      for (const [id, fixture] of loaded.fixtures.entries()) {
        const { id: _, ...result } = round0.results.find((r) => r.id === id) ?? { id };
        expect(fixture.result).toEqual(id < 10 ? result : null);
      }
      expect(await ask<SaveInfo[]>(page, 'save.list')).toMatchObject([{ id: save.id, day: 7 }]);

      // The round, as the screens ask for it.
      const round = await ask<{ id: number; home: number; away: number; result: Result | null }[]>(page, 'world.round', {
        round: 0,
      });
      expect(round.map((m) => m.id)).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
      expect(round[4]).toMatchObject({ home: world.fixtures[4]!.home, away: world.fixtures[4]!.away });
      expect(round.every((m) => m.result !== null)).toBe(true);
      expect((await ask<{ result: Result | null }[]>(page, 'world.round', { round: 1 })).every((m) => m.result === null)).toBe(
        true,
      );

      // The table is the results added up, worked out here independently.
      type Row = { club: number; played: number; won: number; drawn: number; lost: number; goalsFor: number; goalsAgainst: number; points: number };
      const expected: Row[] = Array.from({ length: CLUBS }, (_, club) => ({
        club,
        played: 0,
        won: 0,
        drawn: 0,
        lost: 0,
        goalsFor: 0,
        goalsAgainst: 0,
        points: 0,
      }));
      for (const r of round0.results) {
        const f = world.fixtures[r.id]!;
        for (const [club, scored, conceded] of [
          [f.home, r.homeGoals, r.awayGoals],
          [f.away, r.awayGoals, r.homeGoals],
        ] as const) {
          const row = expected[club]!;
          row.played += 1;
          row.goalsFor += scored;
          row.goalsAgainst += conceded;
          if (scored > conceded) row.won += 1;
          else if (scored === conceded) row.drawn += 1;
          else row.lost += 1;
          row.points = 3 * row.won + row.drawn;
        }
      }
      expected.sort(
        (a, b) =>
          b.points - a.points ||
          b.goalsFor - b.goalsAgainst - (a.goalsFor - a.goalsAgainst) ||
          b.goalsFor - a.goalsFor ||
          a.club - b.club,
      );
      const table = await ask<(Row & { name: string; shortName: string })[]>(page, 'world.standings');
      expect(table.map(({ name: _n, shortName: _s, ...row }) => row)).toEqual(expected);
      expect(table.every((row) => row.name === world.clubs[row.club]!.name)).toBe(true);
      expect(table.reduce((sum, row) => sum + row.played, 0)).toBe(20);
    });

    test('a worker killed in the middle of ending a day: the save is on the day before, whole', async ({
      page,
    }, testInfo) => {
      await open(page, testInfo.project.name);
      const { save, world } = await saveWithWorld(page, 'Crash no dia');
      for (let day = 1; day <= 6; day += 1) await commitDay(page, makeCommit(world, day));
      const onDay6 = await ask<string>(page, 'save.digest');

      // The round is written, the transaction never commits, the worker dies.
      await start(page, { test: { stopCommitDay: true } });
      await ask(page, 'save.open', { id: save.id });
      expect(await commitDay(page, makeCommit(world, 7), false)).toBe('not awaited');
      await start(page);
      expect(await digestOf(page, save.id)).toBe(onDay6);
      const back = (await loadWorld(page)) as World;
      expect(back.day).toBe(6);
      expect(back.fixtures.every((f) => f.result === null)).toBe(true);
      expect(await ask<SaveInfo[]>(page, 'save.list')).toMatchObject([{ id: save.id, day: 6 }]);

      // The day can be ended again, and now it stays.
      expect(await commitDay(page, makeCommit(world, 7))).toBeNull();
      await start(page);
      await ask(page, 'save.open', { id: save.id });
      expect(((await loadWorld(page)) as World).day).toBe(7);
    });

    test('a save with a world exports and imports with the same content; its size', async ({ page }, testInfo) => {
      await open(page, testInfo.project.name);
      const { save, world } = await saveWithWorld(page, 'Vai e volta');
      for (let day = 1; day <= 14; day += 1) await commitDay(page, makeCommit(world, day));
      const digest = await ask<string>(page, 'save.digest');
      const exported = await exportSave(page, save.id);
      console.log(
        `[7B save size ${testInfo.project.name} ${backend}] ${exported.bytes.length} bytes (${(exported.bytes.length / 1024).toFixed(0)} kB) with the world, two rounds played`,
      );
      // Well inside what is cheap to rewrite whole every day (IndexedDB).
      expect(exported.bytes.length).toBeLessThan(1024 * 1024);

      const other = await ask<SaveInfo>(page, 'save.create', { name: 'Recebe' });
      await importSave(page, other.id, exported.bytes);
      expect(await digestOf(page, other.id)).toBe(digest);
      const copy = (await loadWorld(page)) as World;
      expect(copy.day).toBe(14);
      expect(copy.seed).toBe('18446744073709551557');
      expect(copy.fixtures.filter((f) => f.result !== null)).toHaveLength(20);
      // The catalog of the receiving save follows the world it now holds.
      expect((await ask<SaveInfo[]>(page, 'save.list')).find((s) => s.id === other.id)).toMatchObject({
        clubName: 'Clube D',
        day: 14,
      });
      // And the copy goes on by itself.
      expect(await commitDay(page, makeCommit(world, 15))).toBeNull();
      expect(await digestOf(page, save.id)).toBe(digest);
    });
  });
}
