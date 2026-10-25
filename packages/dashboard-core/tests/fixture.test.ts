import { expect, test } from 'bun:test';

test('fixture sources do not encode consumer products', async () => {
  const sources = new Bun.Glob('**/*.{ts,tsx}');
  for await (const path of sources.scan({ cwd: `${import.meta.dir}/../fixture` })) {
    const source = await Bun.file(`${import.meta.dir}/../fixture/${path}`).text();
    expect(source).not.toMatch(/hvac|intercom|cn105|thermostat|doorlock|door_locked/i);
  }
});

test('neutral scenarios retain independent themes and presentation coverage', async () => {
  expect(await Bun.file(`${import.meta.dir}/../fixture/scenarios.ts`).exists()).toBe(true);
  const { fixtureScenarios } = await import('../fixture/scenarios');
  expect(fixtureScenarios.length).toBeGreaterThan(1);
  expect(new Set(fixtureScenarios.map((scenario) => scenario.themeStorageKey)).size).toBe(
    fixtureScenarios.length
  );
  const readiness = fixtureScenarios.flatMap((scenario) =>
    scenario.ecosystem.endpoints?.map((endpoint) => endpoint.ready)
  );
  expect(readiness).toContain(true);
  expect(readiness).toContain(false);
  const subscriptions = fixtureScenarios.flatMap((scenario) =>
    scenario.subscriptionRows.map((row) => scenario.ecosystem.properties[row.propertyKey])
  );
  expect(subscriptions).toContain(true);
  expect(subscriptions).toContain(false);
  expect(subscriptions).toContain(undefined);
});

test('fixture supplies ecosystem strategies through EcosystemStrategy interface', async () => {
  const fixture = await Bun.file(`${import.meta.dir}/../fixture/main.tsx`).text();
  const ecosystemTab = await Bun.file(`${import.meta.dir}/../src/EcosystemTab.tsx`).text();

  expect(fixture).toContain('ecosystemStrategies={');
  expect(fixture).not.toContain('supplementalPanel={');
  expect(ecosystemTab).toContain('props.strategies');
  expect(ecosystemTab).not.toContain('supplementalPanel');
});
