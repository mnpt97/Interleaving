import { afterEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { saveScenarioJson } from './tauri';
import source from '../../../../scenarios/examples/hello-api.json?raw';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => false }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: vi.fn(), open: vi.fn() }));
afterEach(() => vi.resetAllMocks());
it('returns the chosen JSON destination and reuses it without reopening the dialog', async () => {
  const validation = { valid: true, document: JSON.parse(source), diagnostics: [] };
  vi.mocked(save).mockResolvedValue('/tmp/scenario.json');
  vi.mocked(invoke).mockResolvedValue(validation);
  const saved = await saveScenarioJson(source, 'hello-api');
  expect(saved).toMatchObject({ path: '/tmp/scenario.json', validation });
  await saveScenarioJson(source, 'hello-api', '/tmp/scenario.json');
  expect(save).toHaveBeenCalledTimes(1);
  expect(invoke).toHaveBeenLastCalledWith('save_scenario_draft_json', { path: '/tmp/scenario.json', source });
});
it('does not write when the destination dialog is cancelled', async () => {
  vi.mocked(save).mockResolvedValue(null);
  expect(await saveScenarioJson(source, 'hello-api')).toBe(false);
  expect(invoke).not.toHaveBeenCalled();
});
