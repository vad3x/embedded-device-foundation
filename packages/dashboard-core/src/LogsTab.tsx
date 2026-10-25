import { RotateCw } from 'lucide-solid';
import { type Component, createSignal, onMount } from 'solid-js';
import { Card, CardBody } from './components/Card';

export interface LogsTabProps {
  loadLogs: () => Promise<string>;
}

export const LogsTab: Component<LogsTabProps> = (props) => {
  const [logs, setLogs] = createSignal('Loading logs...');
  const [loading, setLoading] = createSignal(false);

  const load = async () => {
    setLoading(true);
    try {
      setLogs(await props.loadLogs());
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setLogs(`Failed to fetch logs: ${message}`);
    } finally {
      setLoading(false);
    }
  };

  onMount(() => {
    void load();
  });

  return (
    <Card bordered>
      <CardBody>
        <div class="flex items-center justify-between mb-3">
          <h3 class="text-sm font-semibold tracking-wider uppercase text-base-content/60 m-0">
            Live System Logs
          </h3>
          <button
            type="button"
            class="btn btn-sm btn-outline gap-2"
            onClick={load}
            disabled={loading()}
          >
            <RotateCw class={`w-4 h-4 ${loading() ? 'animate-spin' : ''}`} />
            Refresh Logs
          </button>
        </div>
        <pre class="bg-base-300 p-4 rounded-lg font-mono text-xs overflow-x-auto max-h-[500px] border border-base-content/10 leading-relaxed text-base-content/90 whitespace-pre-wrap select-text">
          {logs()}
        </pre>
      </CardBody>
    </Card>
  );
};
