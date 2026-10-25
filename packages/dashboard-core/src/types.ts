import type { Component } from 'solid-js';

export interface FabricInfo {
  fab_idx: number;
  vendor_id: number;
  fabric_id: string;
  node_id: string;
  label: string;
}

export interface EndpointInfo {
  id: number;
  name: string;
  cluster: string;
  ready: boolean;
}

export interface ActiveEcosystem {
  type: string;
  paired: boolean;
  properties: Record<string, boolean | number | string | undefined>;
  fabrics?: FabricInfo[];
  endpoints?: EndpointInfo[];
}

export interface SystemDiagnosticsBase {
  device_type: string;
  state: string;
  uptime_sec: number;
  wifi_ssid: string | null;
  wifi_rssi_dbm: number | null;
  ip: string | null;
  mac: string;
  active_slot: string;
  slot_valid: boolean;
  free_heap_bytes: number;
  total_heap_bytes: number;
  available_ecosystems: string[];
  active_ecosystem: ActiveEcosystem | null;
  version: string;
  git_commit: string;
  build_date: string;
}

export interface SubscriptionRow {
  label: string;
  propertyKey: string;
  endpointPropertyKey?: string;
  endpointId?: number;
}

export interface EcosystemStrategyProps {
  ecosystem: ActiveEcosystem | null | undefined;
  status?: SystemDiagnosticsBase;
}

export interface EcosystemStrategy {
  id: string;
  label: string;
  icon?: Component<{ class?: string }>;
  Component: Component<EcosystemStrategyProps>;
}
