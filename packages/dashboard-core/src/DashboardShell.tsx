import { Activity, Radio, Sliders, Terminal } from 'lucide-solid';
import { type Component, createSignal, For, type JSX, Match, Switch } from 'solid-js';
import { Navbar } from './components/Navbar';
import { DiagnosticsTab } from './DiagnosticsTab';
import { EcosystemTab } from './EcosystemTab';
import { LogsTab } from './LogsTab';
import type { EcosystemStrategy, SystemDiagnosticsBase } from './types';

export interface DashboardShellProps {
  title: string;
  brandIcon: JSX.Element;
  badges: JSX.Element;
  controlTab: JSX.Element;
  status?: SystemDiagnosticsBase | null;
  themeStorageKey: string;
  ecosystemStrategies: readonly EcosystemStrategy[];
  tabOrder: readonly [DashboardTabId, ...DashboardTabId[]];
  loadLogs: () => Promise<string>;
  toast?: JSX.Element;
  onReboot?: () => Promise<void>;
  onFactoryReset?: () => Promise<void>;
}

export type DashboardTabId = 'controls' | 'ecosystem' | 'diagnostics' | 'logs';

const tabs = {
  controls: { label: 'Controls', icon: Sliders },
  ecosystem: { label: 'Ecosystem', icon: Radio },
  diagnostics: { label: 'Diagnostics', icon: Activity },
  logs: { label: 'Logs', icon: Terminal },
} as const;

export const DashboardShell: Component<DashboardShellProps> = (props) => {
  const [activeTab, setActiveTab] = createSignal<DashboardTabId>(props.tabOrder[0]);
  return (
    <div class="min-h-screen flex flex-col bg-base-300">
      <Navbar
        title={props.title}
        brandIcon={props.brandIcon}
        badges={props.badges}
        status={props.status}
        themeStorageKey={props.themeStorageKey}
      />
      <nav class="bg-base-200 border-b border-base-content/10 px-4 sm:px-6">
        <div class="flex gap-1 overflow-x-auto max-w-4xl mx-auto">
          <For each={props.tabOrder}>
            {(tabId) => {
              const tab = tabs[tabId];
              const Icon = tab.icon;
              const active = () => activeTab() === tabId;
              return (
                <button
                  type="button"
                  class={`flex items-center gap-2 py-3 px-4 font-semibold text-sm border-b-2 transition-colors whitespace-nowrap ${active() ? 'border-primary text-primary' : 'border-transparent text-base-content/60 hover:text-base-content'}`}
                  onClick={() => setActiveTab(tabId)}
                >
                  <Icon class="w-4 h-4" />
                  {tab.label}
                </button>
              );
            }}
          </For>
        </div>
      </nav>
      <main class="flex-1 max-w-4xl w-full mx-auto p-4 sm:p-6">
        <Switch>
          <Match when={activeTab() === 'controls'}>{props.controlTab}</Match>
          <Match when={activeTab() === 'ecosystem'}>
            <EcosystemTab status={props.status} strategies={props.ecosystemStrategies} />
          </Match>
          <Match when={activeTab() === 'diagnostics'}>
            <DiagnosticsTab
              status={props.status}
              onReboot={props.onReboot}
              onFactoryReset={props.onFactoryReset}
            />
          </Match>
          <Match when={activeTab() === 'logs'}>
            <LogsTab loadLogs={props.loadLogs} />
          </Match>
        </Switch>
      </main>
      {props.toast}
    </div>
  );
};
