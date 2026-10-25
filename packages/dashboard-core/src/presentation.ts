import type { EndpointInfo, SubscriptionRow } from './types';

export type BadgeVariant = 'success' | 'warning' | 'error' | 'info' | 'ghost' | 'neutral';

export function subscriptionState(value: boolean | undefined): {
  label: string;
  variant: BadgeVariant;
} {
  switch (value) {
    case true:
      return { label: 'Subscribed', variant: 'success' };
    case false:
      return { label: 'Waiting', variant: 'warning' };
    default:
      return { label: 'Ready', variant: 'neutral' };
  }
}

export function endpointRows(
  endpoints: EndpointInfo[] | undefined,
  fallback: readonly SubscriptionRow[],
  properties: Record<string, boolean | number | string | undefined>
):
  | { kind: 'endpoints'; rows: EndpointInfo[] }
  | {
      kind: 'subscriptions';
      rows: Array<{
        label: string;
        endpointId?: number;
        state: ReturnType<typeof subscriptionState>;
      }>;
    } {
  if (endpoints?.length) return { kind: 'endpoints', rows: endpoints };
  return {
    kind: 'subscriptions',
    rows: fallback.map(({ label, propertyKey, endpointPropertyKey, endpointId }) => {
      const epKey = endpointPropertyKey ?? `${propertyKey.replace(/_subscribed$/, '')}_endpoint`;
      const propEp =
        typeof properties[epKey] === 'number' ? (properties[epKey] as number) : undefined;
      return {
        label,
        endpointId: propEp ?? endpointId,
        state: subscriptionState(
          typeof properties[propertyKey] === 'boolean'
            ? (properties[propertyKey] as boolean)
            : undefined
        ),
      };
    }),
  };
}

export function formatUptime(sec: number | undefined): string {
  if (sec == null) return '--';
  const days = Math.floor(sec / 86400);
  const hrs = Math.floor((sec % 86400) / 3600);
  const mins = Math.floor((sec % 3600) / 60);
  const secs = sec % 60;
  if (days > 0) return `${days}d ${hrs}h ${mins}m ${secs}s`;
  if (hrs > 0) return `${hrs}h ${mins}m ${secs}s`;
  if (mins > 0) return `${mins}m ${secs}s`;
  return `${secs}s`;
}

export function heapInfo(
  free: number | undefined,
  total: number | undefined
): {
  usedKb: string;
  totalKb: string;
  pct: number;
} {
  if (total == null || !Number.isFinite(total) || total <= 0) {
    return { usedKb: '0', totalKb: '0', pct: 0 };
  }
  const available = Number.isFinite(free) ? Math.min(total, Math.max(0, free ?? 0)) : 0;
  const used = total - available;
  return {
    usedKb: (used / 1024).toFixed(1),
    totalKb: (total / 1024).toFixed(1),
    pct: Math.round((used / total) * 100),
  };
}

const backgroundClasses = {
  'base-100': 'bg-base-100',
  'base-200': 'bg-base-200',
  'base-300': 'bg-base-300',
  neutral: 'bg-neutral',
} as const;

export type CardBackground = keyof typeof backgroundClasses;

export function cardBackgroundClass(background: CardBackground | undefined): string {
  return backgroundClasses[background ?? 'base-200'];
}

export function themeFromStorage(
  storage: { getItem: (key: string) => string | null },
  key: string
): 'night' | 'light' {
  try {
    return storage.getItem(key) === 'light' ? 'light' : 'night';
  } catch {
    return 'night';
  }
}

export function resetPhase(
  hasCallback: boolean,
  confirming: boolean,
  pending: boolean
): 'hidden' | 'action' | 'confirmation' | 'pending' {
  if (!hasCallback) return 'hidden';
  if (!confirming) return 'action';
  return pending ? 'pending' : 'confirmation';
}
