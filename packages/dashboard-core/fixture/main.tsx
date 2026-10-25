import {
  Badge,
  Card,
  CardBody,
  createDashboardApi,
  DashboardShell,
  type EcosystemStrategy,
  MatterEcosystemPanel,
  type SystemDiagnosticsBase,
  Toast,
} from '@embedded-device-foundation/dashboard-core';
import { Activity } from 'lucide-solid';
import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import './index.css';
import { baseStatus, type FixtureScenarios, fixtureScenarios } from './scenarios';

const fakeFetch = (async (url: string, init?: RequestInit) => {
  if (url === '/api/logs')
    return new Response('12:00:01 [INFO] Fixture ready\n12:00:02 [INFO] Matter connected');
  if (url === '/api/action/reset/matter' && init?.method === 'POST')
    return new Response(null, { status: 204 });
  if (url === '/api/status') return Response.json(baseStatus);
  return new Response('Unknown fixture route', { status: 404 });
}) as typeof fetch;
const api = createDashboardApi({ fetchImpl: fakeFetch });

function Fixture(props: { scenarios: FixtureScenarios }) {
  const [scenarioIndex, setScenarioIndex] = createSignal(0);
  const scenario = () => props.scenarios[scenarioIndex()];
  const [loaded, setLoaded] = createSignal(true);
  const [withEndpoints, setWithEndpoints] = createSignal(false);
  const [resetEnabled, setResetEnabled] = createSignal(true);
  const [resetting, setResetting] = createSignal(false);
  const [toast, setToast] = createSignal<string | null>(null);
  const status = (): SystemDiagnosticsBase | null => {
    if (!loaded()) return null;
    return {
      ...baseStatus,
      active_ecosystem: {
        ...scenario().ecosystem,
        endpoints: withEndpoints() ? scenario().ecosystem.endpoints : [],
      },
    };
  };
  const reset = async () => {
    setResetting(true);
    try {
      await new Promise((resolve) => setTimeout(resolve, 3500));
      await api.resetMatterKvs();
      setToast('Fixture reset request sent');
    } finally {
      setResetting(false);
    }
  };

  const strategies: readonly EcosystemStrategy[] = [
    {
      id: 'matter',
      label: 'Matter over Wi-Fi',
      Component: (p) => (
        <MatterEcosystemPanel
          ecosystem={p.ecosystem}
          subscriptionRows={scenario().subscriptionRows}
          endpointIcon={(ep) => (ep.id === 1 ? '🔒 ' : '🔔 ')}
          onResetMatter={resetEnabled() ? reset : undefined}
          isResettingMatter={resetting()}
        />
      ),
    },
  ];

  return (
    <>
      <div class="bg-base-100 p-3 flex flex-wrap gap-2 items-center justify-center">
        <button
          type="button"
          class="btn btn-sm"
          onClick={() => setScenarioIndex((index) => (index + 1) % props.scenarios.length)}
        >
          Scenario: {scenario().title}
        </button>
        <button type="button" class="btn btn-sm" onClick={() => setLoaded(!loaded())}>
          Status: {loaded() ? 'loaded' : 'null'}
        </button>
        <button type="button" class="btn btn-sm" onClick={() => setWithEndpoints(!withEndpoints())}>
          Rows: {withEndpoints() ? 'endpoints' : 'fallback'}
        </button>
        <button type="button" class="btn btn-sm" onClick={() => setResetEnabled(!resetEnabled())}>
          Reset: {resetEnabled() ? 'enabled' : 'read-only'}
        </button>
      </div>
      <DashboardShell
        title={scenario().title}
        brandIcon={<Activity class="w-5 h-5" />}
        badges={<Badge variant={loaded() ? 'success' : 'warning'}>{scenario().badge}</Badge>}
        ecosystemStrategies={strategies}
        controlTab={
          <Card bordered>
            <CardBody>
              <h2>Fixture controls</h2>
              <p>No device commands are sent by this fixture.</p>
            </CardBody>
          </Card>
        }
        status={status()}
        themeStorageKey={scenario().themeStorageKey}
        tabOrder={['controls', 'diagnostics', 'ecosystem', 'logs']}
        loadLogs={api.fetchLogs}
        toast={<Toast message={toast()} type="info" />}
      />
    </>
  );
}

const root = document.getElementById('root');
if (!root) throw new Error('Fixture root missing');
render(() => <Fixture scenarios={fixtureScenarios} />, root);
