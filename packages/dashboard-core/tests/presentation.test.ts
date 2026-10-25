import { describe, expect, test } from 'bun:test';
import {
  cardBackgroundClass,
  endpointRows,
  formatUptime,
  heapInfo,
  resetPhase,
  subscriptionState,
  themeFromStorage,
} from '../src/presentation';

describe('shared presentation rules', () => {
  test('subscription true, false and absent retain their distinct labels', () => {
    expect(subscriptionState(true)).toEqual({ label: 'Subscribed', variant: 'success' });
    expect(subscriptionState(false)).toEqual({ label: 'Waiting', variant: 'warning' });
    expect(subscriptionState(undefined)).toEqual({ label: 'Ready', variant: 'neutral' });
  });

  test('endpoint rows take precedence over configured fallback subscriptions', () => {
    const fallback = [{ label: 'Channel A', propertyKey: 'sample_a_ready' }];
    const endpoints = [{ id: 1, name: 'Channel A', cluster: 'ExampleCluster', ready: true }];
    expect(endpointRows(endpoints, fallback, { sample_a_ready: false })).toEqual({
      kind: 'endpoints',
      rows: endpoints,
    });
    expect(endpointRows([], fallback, { sample_a_ready: false })).toEqual({
      kind: 'subscriptions',
      rows: [
        {
          label: 'Channel A',
          endpointId: undefined,
          state: { label: 'Waiting', variant: 'warning' },
        },
      ],
    });
  });

  test('endpoint rows extract endpointId from properties or explicit row configuration', () => {
    const fallback = [
      { label: 'Thermostat', propertyKey: 'thermostat_subscribed' },
      { label: 'Custom', propertyKey: 'custom_ready', endpointPropertyKey: 'custom_ep' },
      { label: 'Explicit', propertyKey: 'explicit_ready', endpointId: 3 },
    ];
    const properties = {
      thermostat_subscribed: true,
      thermostat_endpoint: 1,
      custom_ready: false,
      custom_ep: 2,
      explicit_ready: true,
    };
    expect(endpointRows([], fallback, properties)).toEqual({
      kind: 'subscriptions',
      rows: [
        { label: 'Thermostat', endpointId: 1, state: { label: 'Subscribed', variant: 'success' } },
        { label: 'Custom', endpointId: 2, state: { label: 'Waiting', variant: 'warning' } },
        { label: 'Explicit', endpointId: 3, state: { label: 'Subscribed', variant: 'success' } },
      ],
    });
  });

  test('uptime handles missing values and days', () => {
    expect(formatUptime(undefined)).toBe('--');
    expect(formatUptime(0)).toBe('0s');
    expect(formatUptime(90061)).toBe('1d 1h 1m 1s');
  });

  test('heap handles missing, invalid, and normal totals', () => {
    expect(heapInfo(undefined, undefined)).toEqual({ usedKb: '0', totalKb: '0', pct: 0 });
    expect(heapInfo(1200, 1000).pct).toBe(0);
    expect(heapInfo(512, 1024)).toEqual({ usedKb: '0.5', totalKb: '1.0', pct: 50 });
    expect(heapInfo(-100, 1024)).toEqual({ usedKb: '1.0', totalKb: '1.0', pct: 100 });
    expect(heapInfo(2048, 1024)).toEqual({ usedKb: '0.0', totalKb: '1.0', pct: 0 });
    expect(heapInfo(Number.NaN, 1024).pct).toBe(100);
    expect(heapInfo(0, Number.POSITIVE_INFINITY)).toEqual({ usedKb: '0', totalKb: '0', pct: 0 });
  });

  test('reset phase hides an unavailable action and retains confirmation while pending', () => {
    expect(resetPhase(false, false, false)).toBe('hidden');
    expect(resetPhase(false, true, true)).toBe('hidden');
    expect(resetPhase(true, false, false)).toBe('action');
    expect(resetPhase(true, true, false)).toBe('confirmation');
    expect(resetPhase(true, true, true)).toBe('pending');
  });

  test('card backgrounds are static Tailwind classes', () => {
    expect(
      (['base-100', 'base-200', 'base-300', 'neutral'] as const).map(cardBackgroundClass)
    ).toEqual(['bg-base-100', 'bg-base-200', 'bg-base-300', 'bg-neutral']);
    expect(cardBackgroundClass(undefined)).toBe('bg-base-200');
  });

  test('theme keys remain isolated and invalid or unavailable values use night', () => {
    const values = new Map([
      ['sample_alpha_theme', 'light'],
      ['sample_beta_theme', 'night'],
    ]);
    const storage = { getItem: (key: string) => values.get(key) ?? null };
    expect(themeFromStorage(storage, 'sample_alpha_theme')).toBe('light');
    expect(themeFromStorage(storage, 'sample_beta_theme')).toBe('night');
    expect(themeFromStorage(storage, 'missing_theme')).toBe('night');
    values.set('sample_beta_theme', 'invalid');
    expect(themeFromStorage(storage, 'sample_beta_theme')).toBe('night');
    expect(
      themeFromStorage(
        {
          getItem: () => {
            throw new Error('restricted');
          },
        },
        'sample_alpha_theme'
      )
    ).toBe('night');
  });
});
