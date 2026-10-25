import { describe, expect, test } from 'bun:test';
import { createDashboardApi } from '../src/api';

describe('common dashboard API', () => {
  test('uses the exact shared status and logs routes', async () => {
    const calls: Array<[string, RequestInit | undefined]> = [];
    const fetchImpl = (async (url: string, init?: RequestInit) => {
      calls.push([url, init]);
      return new Response(url.endsWith('/logs') ? 'booted' : '{"state":"Running"}');
    }) as typeof fetch;
    const api = createDashboardApi({ fetchImpl, baseUrl: 'http://fixture.test/' });

    expect((await api.fetchStatus()).state).toBe('Running');
    expect(await api.fetchLogs()).toBe('booted');
    expect(calls).toEqual([
      ['http://fixture.test/api/status', undefined],
      ['http://fixture.test/api/logs', undefined],
    ]);
  });

  test('posts the exact Matter reset route', async () => {
    const calls: Array<[string, RequestInit | undefined]> = [];
    const fetchImpl = (async (url: string, init?: RequestInit) => {
      calls.push([url, init]);
      return new Response(null, { status: 204 });
    }) as typeof fetch;

    await createDashboardApi({ fetchImpl }).resetMatterKvs();
    expect(calls).toEqual([['/api/action/reset/matter', { method: 'POST' }]]);
  });

  test('posts the exact reboot and factory reset routes', async () => {
    const calls: Array<[string, RequestInit | undefined]> = [];
    const fetchImpl = (async (url: string, init?: RequestInit) => {
      calls.push([url, init]);
      return new Response(null, { status: 200 });
    }) as typeof fetch;

    const api = createDashboardApi({ fetchImpl });
    await api.reboot();
    await api.factoryReset();
    expect(calls).toEqual([
      ['/api/action/reboot', { method: 'POST' }],
      ['/api/action/reset/factory', { method: 'POST' }],
    ]);
  });

  test.each(['fetchStatus', 'fetchLogs', 'resetMatterKvs', 'reboot', 'factoryReset'] as const)(
    '%s propagates HTTP failures',
    async (method) => {
      const fetchImpl = (async () => new Response('failed', { status: 503 })) as typeof fetch;
      await expect(createDashboardApi({ fetchImpl })[method]()).rejects.toThrow('HTTP 503');
    }
  );
});
