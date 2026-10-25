import type { SystemDiagnosticsBase } from './types';

export interface DashboardApiOptions {
  fetchImpl?: typeof fetch;
  baseUrl?: string;
}

export function createDashboardApi<T extends SystemDiagnosticsBase = SystemDiagnosticsBase>(
  options: DashboardApiOptions = {}
) {
  const fetchImpl = options.fetchImpl ?? fetch;
  const baseUrl = options.baseUrl?.replace(/\/$/, '') ?? '';

  const request = async (path: string, init?: RequestInit): Promise<Response> => {
    const response = await fetchImpl(`${baseUrl}${path}`, init);
    if (!response.ok) throw new Error(`HTTP ${response.status}: ${path}`);
    return response;
  };

  return {
    async fetchStatus(): Promise<T> {
      return (await request('/api/status')).json() as Promise<T>;
    },
    async fetchLogs(): Promise<string> {
      return (await request('/api/logs')).text();
    },
    async resetMatterKvs(): Promise<void> {
      await request('/api/action/reset/matter', { method: 'POST' });
    },
    async reboot(): Promise<void> {
      await request('/api/action/reboot', { method: 'POST' });
    },
    async factoryReset(): Promise<void> {
      await request('/api/action/reset/factory', { method: 'POST' });
    },
  };
}
