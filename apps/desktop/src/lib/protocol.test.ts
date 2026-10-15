import { beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { startScenario } from './tauri';
import { type RunMessage } from './run-types';
import reusable from '../../../../scenarios/examples/reusable-auth.interl.json?raw';
import { openScenarioFile } from './tauri';
import { open } from '@tauri-apps/plugin-dialog';
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  isTauri: () => false,
  Channel: class {
    onmessage: ((message: unknown) => void) | undefined;
  },
}));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }));
beforeEach(() => vi.clearAllMocks());
it('rejects an early incompatible protocol safely and cancels when the run ID arrives', async () => {
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === 'start_scenario') {
      (args as { channel: { onmessage: (message: unknown) => void } }).channel.onmessage({
        protocolVersion: 2,
        kind: 'event',
        event: { private: 'private-marker' },
      });
      return { runId: 'run', protocolVersion: 3 };
    }
    return true;
  });
  const messages: RunMessage[] = [];
  await startScenario('{}', (message) => messages.push(message));
  expect(messages).toEqual([
    { protocolVersion: 3, kind: 'failed', message: 'Execution protocol is incompatible. Restart the desktop app.' },
  ]);
  expect(invoke).toHaveBeenCalledWith('cancel_scenario', { runId: 'run' });
  expect(JSON.stringify(messages)).not.toContain('private-marker');
});
it('rejects an incompatible start response without echoing its contents', async () => {
  vi.mocked(invoke).mockResolvedValue({ runId: 'run', protocolVersion: 2, private: 'private-marker' });
  await expect(startScenario('{}', () => {})).rejects.toMatchObject({ code: 'executionProtocol' });
  expect(invoke).toHaveBeenCalledWith('cancel_scenario', { runId: 'run' });
});
it('opens v2 without stripping reusable workflows or lifecycle bindings', async () => {
  vi.mocked(open).mockResolvedValue('/reusable.interl.json');
  vi.mocked(invoke).mockResolvedValue({
    path: '/reusable.interl.json',
    format: 'json',
    source: reusable,
    validation: { valid: true, document: JSON.parse(reusable), diagnostics: [] },
  });
  const result = await openScenarioFile();
  expect(result?.validation.document?.workflows).toHaveLength(3);
  expect(result?.validation.document?.groups[0].lifecycle.iteration.inputs.token).toEqual({
    source: 'lifecycle',
    phase: 'initialize',
    name: 'token',
  });
});
