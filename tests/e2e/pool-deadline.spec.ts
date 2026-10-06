import { expect, test } from '@playwright/test';
import { resultDeadlineMs } from '../../frontend/src/world/pool';

// Fase 7B (7B.4c): how long the pool waits for the result of a match that
// a match worker confirmed. Ten times the median of the last matches, never
// under 3 s, never over the ceiling; the ceiling while no match was played.
// A pure function: no page is opened.

const CEILING = 30_000;

test.describe('Fase 7B: the deadline of a match follows the matches played', () => {
  test('no match played yet: the ceiling', () => {
    expect(resultDeadlineMs([], CEILING)).toBe(CEILING);
    expect(resultDeadlineMs([], 8_000)).toBe(8_000);
  });

  test('fast matches: the floor of 3 s', () => {
    // Chromium and WebKit in the CI: 90 to 150 ms a match.
    expect(resultDeadlineMs([91], CEILING)).toBe(3_000);
    expect(resultDeadlineMs([142, 150, 134, 160, 139], CEILING)).toBe(3_000);
    expect(resultDeadlineMs([299.9], CEILING)).toBe(3_000);
  });

  test('slower matches: ten times the median', () => {
    // Firefox in the CI: about 1.3 s a match.
    expect(resultDeadlineMs([1_308], CEILING)).toBe(13_080);
    expect(resultDeadlineMs([1_000, 1_400, 1_200], CEILING)).toBe(12_000);
    // An even count: the mean of the two in the middle.
    expect(resultDeadlineMs([1_000, 1_400, 1_200, 1_300], CEILING)).toBe(12_500);
  });

  test('the median, not the mean: one slow match does not stretch the deadline', () => {
    expect(resultDeadlineMs([100, 100, 100, 100, 25_000], CEILING)).toBe(3_000);
    expect(resultDeadlineMs([500, 500, 9_000], CEILING)).toBe(5_000);
  });

  test('a very slow machine: followed up to the ceiling, never past it', () => {
    expect(resultDeadlineMs([2_900], CEILING)).toBe(29_000);
    expect(resultDeadlineMs([3_000], CEILING)).toBe(CEILING);
    expect(resultDeadlineMs([12_000, 15_000], CEILING)).toBe(CEILING);
  });

  test('a ceiling under the floor wins: nothing is waited for longer than the ceiling', () => {
    expect(resultDeadlineMs([100], 2_000)).toBe(2_000);
  });
});
