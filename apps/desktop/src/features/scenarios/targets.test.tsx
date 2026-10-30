import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import legacySource from '../../../../../tests/fixtures/editor/editor-auth.json?raw';
import { documentSchema } from '../../lib/scenario-types';
import { readAuthentication } from './authentication';
import { executeCommand, newDraft } from './draft';
import { effectiveUrl, ScenarioBaseUrl, scenarioBaseUrl, validBaseUrl } from './targets';
import { useScenarioStore } from './store';
const initial = useScenarioStore.getState();
afterEach(() => useScenarioStore.setState(initial, true));
it('uses the selected client profile default, with prefix-preserving overrides', () => {
  const document = newDraft();
  document.clientProfiles[0].baseUrl = 'https://scenario.example/v1';
  expect(effectiveUrl(document, undefined, '/users')).toBe('https://scenario.example/v1/users');
  expect(effectiveUrl(document, 'https://node.example/v2/', '/login')).toBe('https://node.example/v2/login');
  document.clientProfiles.push({ id: 'other', baseUrl: 'https://selected.example/api', timeoutMs: 5000, headers: {} });
  document.groups[0].clientProfileId = 'other';
  expect(scenarioBaseUrl(document)).toBe('https://selected.example/api');
  expect(effectiveUrl(document, undefined, '/users')).toBe('https://selected.example/api/users');
  expect(effectiveUrl(document, undefined, '/../escape')).toContain('Path must stay');
  for (const value of [
    'relative',
    'ftp://example.com',
    'https://user:secret@example.com',
    'https://example.com?query=x',
  ])
    expect(validBaseUrl(value)).toBe(false);
});
it('edits the scenario URL through undoable commands and keeps invalid fragments outside the document', async () => {
  const user = userEvent.setup();
  const document = newDraft();
  useScenarioStore.setState({ document, mode: 'edit', past: [], future: [], formErrors: {} });
  render(<ScenarioBaseUrl />);
  const input = screen.getByRole('textbox', { name: 'Base URL' });
  await user.clear(input);
  await user.type(input, 'invalid');
  expect(useScenarioStore.getState().document!.clientProfiles[0].baseUrl).toBe(document.clientProfiles[0].baseUrl);
  expect(useScenarioStore.getState().formErrors).not.toEqual({});
  await user.clear(input);
  await user.type(input, 'https://api.example/v1');
  expect(useScenarioStore.getState().document!.clientProfiles[0].baseUrl).toBe('https://api.example/v1');
  expect(useScenarioStore.getState().formErrors).toEqual({});
});
it('shows the inherited base URL without inputs in View mode', () => {
  useScenarioStore.setState({ document: newDraft(), mode: 'view' });
  render(<ScenarioBaseUrl />);
  expect(screen.getByText('http://localhost:8080')).toBeInTheDocument();
  expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
});
it('converts a legacy authentication recipe without changing workload credentials or another call', () => {
  const document = documentSchema.parse(JSON.parse(legacySource));
  const node = document.workflows[0].nodes[1];
  if (node.type !== 'call') throw new Error('expected legacy call');
  const credentials = structuredClone(document.identities);
  const groups = structuredClone(document.groups);
  const settings = readAuthentication(document, node, document.workflows[1]);
  const next = executeCommand(document, {
    type: 'authentication.configure',
    workflowId: document.workflows[0].id,
    nodeId: node.id,
    settings: { ...settings, usernameEnv: 'OTHER_USER' },
  });
  expect(next.workflows[0].nodes[1].type).toBe('authentication');
  expect(next.workflows).toHaveLength(1);
  expect(next.identities).toEqual(credentials);
  expect(next.groups).toEqual(groups);
  const other = structuredClone(document.workflows[0]);
  other.id = 'other';
  document.workflows.push(other);
  const shared = executeCommand(document, {
    type: 'authentication.configure',
    workflowId: document.workflows[0].id,
    nodeId: node.id,
    settings,
  });
  expect(shared.workflows.find((w) => w.id === document.workflows[1].id)).toBeDefined();
  expect(shared.workflows.find((w) => w.id === 'other')!.nodes[1].type).toBe('call');
});
