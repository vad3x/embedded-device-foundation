import { RotateCcw, RotateCw, TriangleAlert } from 'lucide-solid';
import { type Component, createSignal, Show } from 'solid-js';
import { Badge } from './components/Badge';
import { Card, CardBody, CardTitle } from './components/Card';
import { formatUptime, heapInfo } from './presentation';
import type { SystemDiagnosticsBase } from './types';

export interface DiagnosticsTabProps {
  status: SystemDiagnosticsBase | null | undefined;
  onReboot?: () => Promise<void>;
  onFactoryReset?: () => Promise<void>;
}

export const DiagnosticsTab: Component<DiagnosticsTabProps> = (props) => {
  const heap = () => heapInfo(props.status?.free_heap_bytes, props.status?.total_heap_bytes);
  const [confirmingReboot, setConfirmingReboot] = createSignal(false);
  const [confirmingReset, setConfirmingReset] = createSignal(false);
  const [pendingAction, setPendingAction] = createSignal<'reboot' | 'reset' | null>(null);

  const isPending = () => pendingAction() !== null;

  const handleReboot = async () => {
    if (!props.onReboot || isPending()) return;
    setPendingAction('reboot');
    try {
      await props.onReboot();
    } finally {
      setPendingAction(null);
      setConfirmingReboot(false);
    }
  };

  const handleFactoryReset = async () => {
    if (!props.onFactoryReset || isPending()) return;
    setPendingAction('reset');
    try {
      await props.onFactoryReset();
    } finally {
      setPendingAction(null);
      setConfirmingReset(false);
    }
  };
  return (
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
      <Card bordered>
        <CardBody>
          <CardTitle>Network &amp; Hardware</CardTitle>
          <div class="overflow-x-auto">
            <table class="table table-sm">
              <tbody>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">System State</td>
                  <td class="text-right font-mono font-semibold text-primary">
                    {props.status?.state || '--'}
                  </td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Uptime</td>
                  <td class="text-right font-mono">{formatUptime(props.status?.uptime_sec)}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">IP Address</td>
                  <td class="text-right font-mono">{props.status?.ip || 'N/A'}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">MAC Address</td>
                  <td class="text-right font-mono">{props.status?.mac || 'N/A'}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Wi-Fi SSID</td>
                  <td class="text-right font-mono">{props.status?.wifi_ssid || 'N/A'}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Wi-Fi RSSI</td>
                  <td class="text-right font-mono">
                    {props.status?.wifi_rssi_dbm != null
                      ? `${props.status.wifi_rssi_dbm} dBm`
                      : 'N/A'}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </CardBody>
      </Card>
      <Card bordered>
        <CardBody>
          <CardTitle>Firmware &amp; Memory</CardTitle>
          <div class="overflow-x-auto">
            <table class="table table-sm">
              <tbody>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Active Slot</td>
                  <td class="text-right font-mono font-bold">
                    {props.status?.active_slot || '--'}
                  </td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Slot Valid</td>
                  <td class="text-right">
                    <Badge
                      variant={
                        !props.status ? 'neutral' : props.status.slot_valid ? 'success' : 'error'
                      }
                    >
                      {!props.status ? 'Loading' : props.status.slot_valid ? 'Valid' : 'Pending'}
                    </Badge>
                  </td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Heap Memory</td>
                  <td class="text-right font-mono">
                    <div>
                      {heap().usedKb} / {heap().totalKb} KB ({heap().pct}%)
                    </div>
                    <progress
                      class="progress progress-primary w-28 h-2 mt-1"
                      value={heap().pct}
                      max="100"
                    />
                  </td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Version</td>
                  <td class="text-right font-mono">{props.status?.version || '--'}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Git Commit</td>
                  <td class="text-right font-mono text-xs">{props.status?.git_commit || '--'}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Build Date</td>
                  <td class="text-right font-mono text-xs">{props.status?.build_date || '--'}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </CardBody>
      </Card>
      <Show when={props.onReboot || props.onFactoryReset}>
        <Card bordered class="md:col-span-2">
          <CardBody>
            <CardTitle>System Controls</CardTitle>
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-4 mt-2">
              <Show when={props.onReboot}>
                <div class="border border-base-content/10 rounded-lg p-4 flex flex-col justify-between space-y-3 bg-base-100/50">
                  <div>
                    <h3 class="font-semibold text-sm">Restart Device</h3>
                    <p class="text-xs text-base-content/70 mt-1">
                      Safely reboot the microcontroller. All configurations and settings are
                      preserved.
                    </p>
                  </div>
                  <Show
                    when={confirmingReboot()}
                    fallback={
                      <button
                        type="button"
                        onClick={() => {
                          setConfirmingReboot(true);
                          setConfirmingReset(false);
                        }}
                        disabled={isPending()}
                        class="btn btn-sm btn-outline btn-warning w-full"
                      >
                        <RotateCw class="w-4 h-4 mr-1.5" /> Restart Device
                      </button>
                    }
                  >
                    <div class="bg-warning/10 border border-warning/30 rounded-lg p-3 space-y-2">
                      <div class="text-xs text-warning font-semibold flex items-center gap-1.5">
                        <TriangleAlert class="w-4 h-4 shrink-0" /> Confirm Restart?
                      </div>
                      <p class="text-[11px] text-base-content/70 leading-normal">
                        This will temporarily disconnect the device while it reboots.
                      </p>
                      <div class="flex gap-2 pt-1">
                        <button
                          type="button"
                          onClick={handleReboot}
                          disabled={isPending()}
                          class="btn btn-xs btn-warning flex-1"
                        >
                          <Show when={pendingAction() === 'reboot'} fallback="Confirm Restart">
                            <span class="loading loading-spinner loading-xs mr-1" />
                            Restarting...
                          </Show>
                        </button>
                        <button
                          type="button"
                          onClick={() => setConfirmingReboot(false)}
                          disabled={isPending()}
                          class="btn btn-xs btn-ghost"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  </Show>
                </div>
              </Show>

              <Show when={props.onFactoryReset}>
                <div class="border border-base-content/10 rounded-lg p-4 flex flex-col justify-between space-y-3 bg-base-100/50">
                  <div>
                    <h3 class="font-semibold text-sm text-error">Factory Reset</h3>
                    <p class="text-xs text-base-content/70 mt-1">
                      Permanently erase all Wi-Fi credentials, paired smart home fabrics, and
                      restore defaults.
                    </p>
                  </div>
                  <Show
                    when={confirmingReset()}
                    fallback={
                      <button
                        type="button"
                        onClick={() => {
                          setConfirmingReset(true);
                          setConfirmingReboot(false);
                        }}
                        disabled={isPending()}
                        class="btn btn-sm btn-outline btn-error w-full"
                      >
                        <RotateCcw class="w-4 h-4 mr-1.5" /> Factory Reset
                      </button>
                    }
                  >
                    <div class="bg-error/10 border border-error/30 rounded-lg p-3 space-y-2">
                      <div class="text-xs text-error font-semibold flex items-center gap-1.5">
                        <TriangleAlert class="w-4 h-4 shrink-0" /> Confirm Factory Reset?
                      </div>
                      <p class="text-[11px] text-base-content/70 leading-normal">
                        This will permanently erase all network configuration and credentials. The
                        device will reboot into setup mode.
                      </p>
                      <div class="flex gap-2 pt-1">
                        <button
                          type="button"
                          onClick={handleFactoryReset}
                          disabled={isPending()}
                          class="btn btn-xs btn-error flex-1"
                        >
                          <Show when={pendingAction() === 'reset'} fallback="Confirm Reset">
                            <span class="loading loading-spinner loading-xs mr-1" />
                            Resetting...
                          </Show>
                        </button>
                        <button
                          type="button"
                          onClick={() => setConfirmingReset(false)}
                          disabled={isPending()}
                          class="btn btn-xs btn-ghost"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  </Show>
                </div>
              </Show>
            </div>
          </CardBody>
        </Card>
      </Show>
    </div>
  );
};
