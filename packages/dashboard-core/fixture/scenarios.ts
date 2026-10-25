import type {
  ActiveEcosystem,
  SubscriptionRow,
  SystemDiagnosticsBase,
} from '@embedded-device-foundation/dashboard-core';

export interface FixtureScenario {
  title: string;
  badge: string;
  themeStorageKey: string;
  subscriptionRows: readonly SubscriptionRow[];
  ecosystem: ActiveEcosystem;
}

export type FixtureScenarios = readonly [FixtureScenario, ...FixtureScenario[]];

export const fixtureScenarios: FixtureScenarios = [
  {
    title: 'Sample Alpha',
    badge: 'Synthetic sample A',
    themeStorageKey: 'fixture_sample_alpha_theme',
    subscriptionRows: [
      { label: 'Channel A (EP1)', propertyKey: 'sample_a_ready' },
      { label: 'Channel B (EP2)', propertyKey: 'sample_b_ready' },
      { label: 'Channel C (EP3)', propertyKey: 'sample_c_ready' },
    ],
    ecosystem: {
      type: 'matter',
      paired: true,
      properties: { sample_a_ready: true, sample_b_ready: false },
      fabrics: [
        { fab_idx: 1, vendor_id: 0xfff1, fabric_id: '0x1', node_id: '0x2', label: 'Sample Hub A' },
      ],
      endpoints: [{ id: 1, name: 'Channel A', cluster: 'ExampleClusterA', ready: true }],
    },
  },
  {
    title: 'Sample Beta',
    badge: 'Synthetic sample B',
    themeStorageKey: 'fixture_sample_beta_theme',
    subscriptionRows: [
      { label: 'Channel X (EP7)', propertyKey: 'sample_x_ready' },
      { label: 'Channel Y (EP8)', propertyKey: 'sample_y_ready' },
    ],
    ecosystem: {
      type: 'matter',
      paired: true,
      properties: { sample_x_ready: true, sample_y_ready: false },
      fabrics: [
        { fab_idx: 1, vendor_id: 0xfff1, fabric_id: '0x3', node_id: '0x4', label: 'Sample Hub B' },
      ],
      endpoints: [{ id: 7, name: 'Channel X', cluster: 'ExampleClusterB', ready: false }],
    },
  },
];

export const baseStatus: SystemDiagnosticsBase = {
  device_type: 'fixture',
  state: 'Running',
  uptime_sec: 90061,
  wifi_ssid: 'Fixture Wi-Fi',
  wifi_rssi_dbm: -58,
  ip: '192.0.2.8',
  mac: '02:00:00:00:00:01',
  active_slot: 'ota_1',
  slot_valid: true,
  free_heap_bytes: 184320,
  total_heap_bytes: 327680,
  available_ecosystems: ['matter'],
  active_ecosystem: null,
  version: '0.1.0',
  git_commit: 'fixture',
  build_date: '2026-10-23',
};
