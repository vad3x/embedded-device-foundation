# Dashboard Core

Shared Solid.js dashboard shell, Matter ecosystem view, diagnostics, logs, UI primitives, and typed common HTTP API. Consumers provide device status, branding, control panels, polling, mock policy, and action callbacks. The package does not send device commands.

## Use

```tsx
import {
  DashboardShell,
  createDashboardApi,
  type SystemDiagnosticsBase,
} from '@embedded-device-foundation/dashboard-core';

interface DeviceStatus extends SystemDiagnosticsBase {
  sample_value: number;
}
const api = createDashboardApi<DeviceStatus>();

<DashboardShell
  title="Example Device"
  brandIcon={<DeviceIcon />}
  badges={<DeviceBadge status={status()} />}
  controlTab={<DeviceControls status={status()} />}
  status={status()}
  themeStorageKey="example_device_theme"
  subscriptionRows={[{ label: 'Channel A (EP1)', propertyKey: 'sample_a_ready' }]}
  tabOrder={['controls', 'ecosystem', 'diagnostics', 'logs']}
  onResetMatter={handleReset}
  isResettingMatter={isResetting()}
  loadLogs={api.fetchLogs}
/>;
```

The common API calls `GET /api/status`, `GET /api/logs`, and `POST /api/action/reset/matter`. Pass `fetchImpl` and `baseUrl` to `createDashboardApi` to use a different transport origin or a test fake. HTTP failures propagate to the consumer.

The development-only fixture uses neutral Sample Alpha/Beta scenarios from `fixture/scenarios.ts`. Its renderer receives the scenario list rather than selecting behavior by product type. These synthetic profiles cover themes, readiness, endpoints, empty status, and fake reset/log callbacks; real product configuration belongs in consumer applications. Neither the fixture nor its scenario data is exported by the library.

Consumers must install one compatible `solid-js` runtime and configure Vite with `vite-plugin-solid`. Their Tailwind CSS must explicitly scan this package source, for example `@source "../../../vendor/embedded-device-foundation/packages/dashboard-core/src";`, using a path relative to their CSS file. Load Tailwind and DaisyUI themes `night` and `light`; no CSS is bundled into the library. The package fixture demonstrates a one-file HTML build that imports the built public export.

From this directory, run `mise exec -- bun install --frozen-lockfile`, `mise exec -- bun run typecheck`, `mise exec -- bun run lint`, `mise exec -- bun test`, `mise exec -- bun run build`, and `mise exec -- bun run build:fixture`.
