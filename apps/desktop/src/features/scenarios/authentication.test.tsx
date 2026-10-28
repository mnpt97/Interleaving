import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import fixtureSource from '../../../../../crates/interl-engine/tests/fixtures/editor-auth-native.json?raw';
import legacySource from '../../../../../crates/interl-engine/tests/fixtures/editor-auth.json?raw';
import { documentSchema } from '../../lib/scenario-types';
import { executeCommand, newDraft } from './draft';
import { authenticationDefinition, authenticationErrors, readNativeAuthentication } from './authentication';
import { useScenarioStore } from './store';
import { AddNodePopover } from './AddNodePopover';
import { NodeInspector } from './NodeInspector';
import { nodeName, presentationFor } from './node-presentation';
import { effectiveUrl } from './targets';
const initial = useScenarioStore.getState();
afterEach(() => useScenarioStore.setState(initial, true));
function inserted() {
  const draft = newDraft();
  draft.id = 'editor-auth';
  return executeCommand(draft, { type: 'authentication.insert', workflowId: 'workflow-1' });
}
function recipe(document = inserted()) {
  const node = document.workflows[0].nodes.find((node) => node.type === 'authentication')!;
  if (node.type !== 'authentication') throw new Error('expected authentication');
  return { document, node, settings: readNativeAuthentication(node) };
}
describe('Authentication nodes', () => {
  it('creates the portable native-node fixture without workload credentials or child workflows', () => {
    const document = inserted();
    expect(document).toEqual(documentSchema.parse(JSON.parse(fixtureSource)));
    expect(document.workflows).toHaveLength(1);
    expect(document.identities).toEqual([]);
    expect(document.groups[0].identityAssignment.identityId).toBeNull();
    expect(nodeName(document.workflows[0].nodes[1], document)).toBe('Authentication');
    expect(presentationFor(document.workflows[0].nodes[1], document).label).toBe('Authentication');
    expect(JSON.stringify(document)).toContain('INTERL_PASSWORD');
  });
  it('keeps two authentication nodes independent when editing credentials', () => {
    const first = inserted();
    const second = executeCommand(first, { type: 'authentication.insert', workflowId: 'workflow-1' });
    const node = second.workflows[0].nodes[1];
    if (node.type !== 'authentication') throw new Error('expected auth');
    const next = executeCommand(second, {
      type: 'authentication.configure',
      workflowId: 'workflow-1',
      nodeId: node.id,
      settings: { ...readNativeAuthentication(node), usernameEnv: 'BOB_USERNAME', passwordEnv: 'BOB_PASSWORD' },
    });
    const previous = next.workflows[0].nodes.find((n) => n.id === 'authenticate-1');
    expect(previous).toEqual(first.workflows[0].nodes[1]);
    expect(next.identities).toEqual([]);
    expect(next.groups).toEqual(first.groups);
    expect(next.workflows).toHaveLength(1);
  });
  it('inserts before requests by default and supports an explicit connection', () => {
    const http = executeCommand(newDraft(), { type: 'http.insert', workflowId: 'workflow-1' });
    const next = executeCommand(http, { type: 'authentication.insert', workflowId: 'workflow-1' });
    expect(next.workflows[0].nodes.map((n) => n.type)).toEqual(['start', 'authentication', 'http', 'end']);
    const explicit = executeCommand(http, {
      type: 'authentication.insert',
      workflowId: 'workflow-1',
      edgeId: http.workflows[0].edges.find((e) => e.to === 'end')!.id,
    });
    expect(explicit.workflows[0].nodes.map((n) => n.type)).toEqual(['start', 'http', 'authentication', 'end']);
  });
  it('roundtrips node-specific base URL, cookie mode and credential mappings', () => {
    const { document, node, settings } = recipe();
    const next = executeCommand(document, {
      type: 'authentication.configure',
      workflowId: 'workflow-1',
      nodeId: node.id,
      settings: {
        ...settings,
        name: 'Device login',
        baseUrl: 'https://device.example/v2',
        mode: 'cookie',
        usernameKey: 'device',
        passwordKey: 'secret',
      },
    });
    const reopened = recipe(documentSchema.parse(JSON.parse(JSON.stringify(next))));
    expect(reopened.settings).toMatchObject({
      name: 'Device login',
      baseUrl: 'https://device.example/v2',
      mode: 'cookie',
      usernameKey: 'device',
    });
    expect(next.workflows[0].edges).toEqual(document.workflows[0].edges);
    expect(effectiveUrl(next, reopened.node.config.baseUrl, '/login')).toBe('https://device.example/v2/login');
    expect(
      effectiveUrl(
        {
          ...next,
          clientProfiles: next.clientProfiles.map((profile) => ({ ...profile, baseUrl: 'https://api.example/v1' })),
        },
        undefined,
        '/users'
      )
    ).toBe('https://api.example/v1/users');
  });
  it('rejects invalid configuration atomically and preserves legacy recipe recognition', () => {
    const { document, node, settings } = recipe();
    const before = structuredClone(document);
    expect(() =>
      executeCommand(document, {
        type: 'authentication.configure',
        workflowId: 'workflow-1',
        nodeId: node.id,
        settings: { ...settings, headerPrefix: 'Bearer\r\nInjected:' },
      })
    ).toThrow('line breaks');
    expect(authenticationErrors({ ...settings, baseUrl: 'https://user:pass@example.com' })).not.toEqual([]);
    expect(document).toEqual(before);
    const legacy = documentSchema.parse(JSON.parse(legacySource));
    expect(authenticationDefinition(legacy, legacy.workflows[0].nodes[1])).not.toBeNull();
  });
  it('undoes insertion as one action and rejects it in View', async () => {
    const before = structuredClone(useScenarioStore.getState().document)!;
    useScenarioStore.getState().command({ type: 'authentication.insert', workflowId: before.workflows[0].id });
    expect(useScenarioStore.getState().document).toEqual(before);
    await useScenarioStore.getState().setMode('edit');
    useScenarioStore.getState().command({ type: 'authentication.insert', workflowId: before.workflows[0].id });
    useScenarioStore.getState().undo();
    expect(useScenarioStore.getState().document).toEqual(before);
    useScenarioStore.getState().redo();
    expect(useScenarioStore.getState().document!.workflows[0].nodes.some((n) => n.type === 'authentication')).toBe(
      true
    );
  });
});
describe('Authentication editor', () => {
  it('adds and configures an independent authentication node with undo and redo', async () => {
    const user = userEvent.setup();
    await useScenarioStore.getState().newScenario();
    render(
      <>
        <AddNodePopover />
        <NodeInspector />
      </>
    );
    await user.click(screen.getByRole('button', { name: 'Add node' }));
    await user.click(screen.getByRole('button', { name: /Authentication/ }));
    expect(screen.queryByRole('combobox', { name: 'Workload identity' })).not.toBeInTheDocument();
    const name = screen.getByRole('textbox', { name: 'Node name' });
    await user.clear(name);
    await user.type(name, 'API login');
    await user.click(screen.getByRole('button', { name: 'Apply configuration' }));
    const env = screen.getByRole('textbox', { name: 'Password environment variable' });
    await user.clear(env);
    await user.type(env, 'INTERL_ALICE_PASSWORD');
    const base = screen.getByRole('textbox', { name: 'Base URL override' });
    await user.type(base, 'https://api.example/v1');
    await user.click(screen.getByRole('button', { name: 'Apply authentication' }));
    expect(recipe(useScenarioStore.getState().document!).settings).toMatchObject({
      name: 'API login',
      passwordEnv: 'INTERL_ALICE_PASSWORD',
      baseUrl: 'https://api.example/v1',
    });
    await waitFor(() => expect(useScenarioStore.getState().formErrors).toEqual({}));
    act(() => useScenarioStore.getState().undo());
    expect(name).toHaveValue('API login');
    act(() => useScenarioStore.getState().undo());
    expect(name).toHaveValue('Authentication');
    act(() => useScenarioStore.getState().redo());
    expect(name).toHaveValue('API login');
    act(() => useScenarioStore.getState().redo());
    await user.selectOptions(screen.getByRole('combobox', { name: 'Authentication mode' }), 'cookie');
    expect(screen.queryByRole('textbox', { name: 'Token expression' })).not.toBeInTheDocument();
  });
  it('keeps viewing read-only', () => {
    const { document, node } = recipe();
    useScenarioStore.setState({ document, selectedWorkflowId: 'workflow-1', selectedId: node.id, mode: 'view' });
    render(<NodeInspector />);
    expect(screen.getByLabelText('Authentication configuration')).toHaveTextContent('Extract token');
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Apply authentication' })).not.toBeInTheDocument();
  });
});
