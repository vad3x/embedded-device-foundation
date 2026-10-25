import { expect, test } from 'bun:test';

test('MatterEcosystemPanel source exports expected interfaces and component', async () => {
  const source = await Bun.file(`${import.meta.dir}/../src/MatterEcosystemPanel.tsx`).text();
  expect(source).toContain('export interface MatterEcosystemPanelProps');
  expect(source).toContain(
    'export const MatterEcosystemPanel: Component<MatterEcosystemPanelProps>'
  );
  expect(source).toContain('Matter over Wi-Fi');
  expect(source).toContain('Reset Matter KVS');
  expect(source).not.toContain('supplementalPanel');
});
