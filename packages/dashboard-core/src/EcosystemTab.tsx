import { type Component, createEffect, createSignal, For, Show } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import type { EcosystemStrategy, SystemDiagnosticsBase } from './types';

export interface EcosystemTabProps {
  status?: SystemDiagnosticsBase | null;
  strategies: readonly EcosystemStrategy[];
}

export const EcosystemTab: Component<EcosystemTabProps> = (props) => {
  const initialId = () => props.status?.active_ecosystem?.type ?? props.strategies[0]?.id;
  const [activeId, setActiveId] = createSignal<string | undefined>(initialId());

  createEffect<string | undefined>((prevType) => {
    const currentType = props.status?.active_ecosystem?.type;
    if (
      currentType &&
      currentType !== prevType &&
      props.strategies.some((s) => s.id === currentType)
    ) {
      setActiveId(currentType);
    }
    return currentType;
  });

  const activeStrategy = () =>
    props.strategies.find((s) => s.id === activeId()) ?? props.strategies[0];

  return (
    <div class="space-y-4">
      <Show when={props.strategies.length > 1}>
        <div class="tabs tabs-boxed bg-base-200 p-1 w-fit">
          <For each={props.strategies}>
            {(strat) => (
              <button
                type="button"
                class={`tab tab-sm font-medium ${activeId() === strat.id ? 'tab-active' : ''}`}
                onClick={() => setActiveId(strat.id)}
              >
                {strat.icon && <strat.icon class="w-4 h-4 mr-1.5" />}
                {strat.label}
              </button>
            )}
          </For>
        </div>
      </Show>
      <Show when={activeStrategy()}>
        {(strat) => (
          <Dynamic
            component={strat().Component}
            ecosystem={props.status?.active_ecosystem}
            status={props.status ?? undefined}
          />
        )}
      </Show>
    </div>
  );
};
