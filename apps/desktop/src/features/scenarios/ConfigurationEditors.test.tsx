import { act, fireEvent, render, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it } from 'vitest';
import { useState } from 'react';
import { executeCommand, newDraft } from './draft';
import { NodeInspector } from './NodeInspector';
import { ValidationPanel } from './ValidationPanel';
import { JsonField, RecordFields, SourceEditor, type Template } from './ConfigurationFields';
import { useScenarioStore } from './store';
const initial = useScenarioStore.getState();
const workflowId = 'workflow-1';
function request() {
  const document = executeCommand(newDraft(), { type: 'http.insert', workflowId });
  const node = document.workflows[0].nodes[1];
  if (node.type !== 'http') throw new Error('HTTP');
  node.config.path = '/health';
  return document;
}
beforeEach(() =>
  useScenarioStore.setState(
    {
      ...initial,
      mode: 'edit',
      document: request(),
      selectedWorkflowId: workflowId,
      selectedId: 'http-1',
      inspectorTab: 'node',
      formErrors: {},
    },
    true
  )
);
afterEach(() => useScenarioStore.setState(initial, true));

it('keeps invalid JSON local, blocks Apply/save/section changes, and discards the entire fragment', async () => {
  const user = userEvent.setup();
  render(<NodeInspector />);
  const before = useScenarioStore.getState().document;
  await user.selectOptions(screen.getByRole('combobox', { name: 'Body format' }), 'json');
  fireEvent.change(screen.getByRole('textbox', { name: 'JSON body' }), { target: { value: '{broken' } });
  expect(screen.getByRole('button', { name: 'Apply configuration' })).toBeDisabled();
  expect(useScenarioStore.getState().document).toBe(before);
  expect(screen.getByRole('tab', { name: 'Scenario' })).toBeDisabled();
  expect(screen.getByRole('tab', { name: 'Workflow' })).toBeDisabled();
  await user.click(screen.getByRole('tab', { name: 'Scenario' }));
  expect(useScenarioStore.getState().error).toBeNull();
  expect(screen.getByRole('tab', { name: 'Node' })).toHaveAttribute('aria-selected', 'true');
  await user.click(screen.getByRole('button', { name: 'Discard fields' }));
  expect(screen.getByRole('combobox', { name: 'Body format' })).toHaveValue('');
  expect(useScenarioStore.getState().formErrors).toEqual({});
  expect(screen.getByRole('tab', { name: 'Scenario' })).toBeEnabled();
  expect(screen.getByRole('tab', { name: 'Workflow' })).toBeEnabled();
});

it('applies HTTP headers, timeout, and literal JSON atomically without interpreting source-looking data', async () => {
  const user = userEvent.setup();
  render(<NodeInspector />);
  await user.click(screen.getByRole('button', { name: 'Add request headers' }));
  await user.type(screen.getByRole('textbox', { name: 'Request headers name 1' }), 'X-Test');
  await user.type(
    within(screen.getByRole('group', { name: 'Request headers' })).getByRole('textbox', { name: 'Value' }),
    'editor'
  );
  fireEvent.change(screen.getByRole('spinbutton', { name: 'Request timeout (ms)' }), { target: { value: '2000' } });
  await user.selectOptions(screen.getByRole('combobox', { name: 'Body format' }), 'json');
  fireEvent.change(screen.getByRole('textbox', { name: 'JSON body' }), {
    target: { value: '{"source":"identity","field":"password"}' },
  });
  await user.click(screen.getByRole('button', { name: 'Apply configuration' }));
  expect(useScenarioStore.getState().document!.workflows[0].nodes[1]).toMatchObject({
    config: {
      timeoutMs: 2000,
      headers: { 'X-Test': 'editor' },
      body: { kind: 'json', value: { source: 'identity', field: 'password' } },
    },
  });
  expect(useScenarioStore.getState().formErrors).toEqual({});
  act(() => useScenarioStore.getState().undo());
  expect(useScenarioStore.getState().document!.workflows[0].nodes[1]).not.toHaveProperty('config.body');
});

it('builds nested object/array templates with typed literal values through controls', async () => {
  const user = userEvent.setup();
  function Editor() {
    const [value, setValue] = useState<Template>({ source: 'literal', value: '' });
    return (
      <>
        <SourceEditor
          template
          value={value}
          onChange={(value) => setValue(value as Template)}
          path="/template"
          scope={{ document: request(), workflow: request().workflows[0] }}
        />
        <output data-testid="template-value">{JSON.stringify(value)}</output>
      </>
    );
  }
  render(<Editor />);
  await user.selectOptions(screen.getByRole('combobox', { name: 'Source' }), 'object');
  await user.click(screen.getByRole('button', { name: 'Add object fields' }));
  await user.type(screen.getByRole('textbox', { name: 'Object fields name 1' }), 'items');
  const group = screen.getByRole('group', { name: 'Object fields' });
  await user.selectOptions(within(group).getByRole('combobox', { name: 'Source' }), 'array');
  await user.click(screen.getByRole('button', { name: 'Add array item' }));
  fireEvent.change(screen.getByRole('textbox', { name: 'Literal JSON value' }), { target: { value: 'false' } });
  expect(JSON.parse(screen.getByTestId('template-value').textContent!)).toEqual({
    source: 'object',
    fields: { items: { source: 'array', items: [{ source: 'literal', value: false }] } },
  });
});

it('configures typed workflow ports with falsy defaults and keeps graph layout intact', async () => {
  const user = userEvent.setup();
  render(<NodeInspector />);
  await user.click(screen.getByRole('tab', { name: 'Workflow' }));
  await user.click(screen.getByRole('button', { name: 'Add input ports' }));
  await user.type(screen.getByRole('textbox', { name: 'Input ports name 1' }), 'enabled');
  await user.selectOptions(screen.getByRole('combobox', { name: 'Input type' }), 'boolean');
  await user.click(screen.getByRole('checkbox', { name: 'Has default' }));
  await user.click(screen.getByRole('button', { name: 'Apply configuration' }));
  expect(useScenarioStore.getState().document!.workflows[0].inputs).toEqual({
    enabled: { type: 'boolean', default: false },
  });
  expect(useScenarioStore.getState().document!.workflows[0].nodes).toHaveLength(3);
  expect(useScenarioStore.getState().formErrors).toEqual({});
});

it('edits scenario variables/profiles/lifecycle without credential-value fields or extra execution options', async () => {
  const user = userEvent.setup();
  render(<NodeInspector />);
  await user.click(screen.getByRole('tab', { name: 'Scenario' }));
  await user.click(screen.getByRole('button', { name: 'Add scenario variables' }));
  await user.type(screen.getByRole('textbox', { name: 'Scenario variables name 1' }), 'resource');
  fireEvent.change(screen.getByRole('textbox', { name: 'Variable JSON value' }), { target: { value: '42' } });
  fireEvent.change(screen.getByRole('spinbutton', { name: 'Profile timeout (ms)' }), { target: { value: '7000' } });
  fireEvent.change(screen.getByRole('spinbutton', { name: 'Cleanup timeout (ms)' }), { target: { value: '4000' } });
  await user.click(screen.getByRole('button', { name: 'Apply configuration' }));
  expect(useScenarioStore.getState().document!.environment.variables).toEqual({ resource: 42 });
  expect(useScenarioStore.getState().document!.clientProfiles[0].timeoutMs).toBe(7000);
  expect(useScenarioStore.getState().document!.groups[0].lifecycle.cleanupTimeoutMs).toBe(4000);
  expect(screen.queryByRole('textbox', { name: /password value|credential value/i })).not.toBeInTheDocument();
  expect(screen.queryByRole('combobox', { name: /ramp|executor/i })).not.toBeInTheDocument();
});

it('opens a diagnostic in the correct workflow and focuses its field despite duplicate local IDs', async () => {
  const document = executeCommand(request(), { type: 'workflow.duplicate', workflowId });
  useScenarioStore.setState({
    document,
    diagnostics: [
      {
        category: 'validation',
        code: 'invalidTimeout',
        message: 'Timeout must be positive.',
        fieldPath: '/workflows/1/nodes/1/config/timeoutMs',
        workflowId: 'workflow-2',
        nodeId: 'http-1',
        filePath: null,
        line: null,
        column: null,
      },
    ],
  });
  render(
    <>
      <ValidationPanel />
      <NodeInspector />
    </>
  );
  fireEvent.click(
    within(screen.getByRole('region', { name: 'Validation diagnostics' })).getByRole('button', {
      name: /Timeout must be positive/,
    })
  );
  expect(useScenarioStore.getState().selectedWorkflowId).toBe('workflow-2');
  await waitFor(() => expect(screen.getByRole('spinbutton', { name: 'Request timeout (ms)' })).toHaveFocus());
});

it('keeps all configuration sections read-only in View', async () => {
  const user = userEvent.setup();
  useScenarioStore.setState({ mode: 'view' });
  render(<NodeInspector />);
  expect(screen.queryByRole('combobox', { name: 'Body format' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('tab', { name: 'Workflow' }));
  expect(screen.queryByRole('button', { name: 'Add input ports' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('tab', { name: 'Scenario' }));
  expect(screen.queryByRole('button', { name: 'Add client profile' })).not.toBeInTheDocument();
});

it('maps a required call input from an earlier HTTP status through reference selectors', async () => {
  const user = userEvent.setup();
  let document = executeCommand(request(), { type: 'node.insert', workflowId, kind: 'call', newWorkflowName: 'Child' });
  document = executeCommand(document, {
    type: 'workflow.configure',
    workflowId: 'workflow-2',
    configuration: { name: 'Child', inputs: { status: { type: 'integer' } }, outputs: {} },
  });
  useScenarioStore.setState({ document, selectedId: 'call-1' });
  render(<NodeInspector />);
  await user.click(screen.getByRole('button', { name: 'Add input mappings' }));
  await user.selectOptions(screen.getByRole('combobox', { name: 'Input mappings name 1' }), 'status');
  await user.selectOptions(
    within(screen.getByRole('group', { name: 'Input mappings' })).getByRole('combobox', { name: 'Source' }),
    'response'
  );
  await user.selectOptions(screen.getByRole('combobox', { name: 'HTTP request' }), 'http-1');
  await user.click(screen.getByRole('button', { name: 'Apply configuration' }));
  expect(useScenarioStore.getState().document!.workflows[0].nodes.find((node) => node.id === 'call-1')).toMatchObject({
    config: { inputs: { status: { source: 'response', nodeId: 'http-1', field: 'status' } } },
  });
});

it('adds environment credential references without changing node-owned Authentication configuration', async () => {
  const user = userEvent.setup();
  const document = executeCommand(newDraft(), { type: 'authentication.insert', workflowId });
  const auth = structuredClone(document.workflows[0].nodes[1]);
  useScenarioStore.setState({ document, selectedId: null, inspectorTab: 'scenario' });
  render(<NodeInspector />);
  await user.click(screen.getByText('Identity references (optional)'));
  await user.click(screen.getByRole('button', { name: 'Add identity references' }));
  await user.click(screen.getByRole('button', { name: 'Add credential references' }));
  await user.type(screen.getByRole('textbox', { name: 'Credential references name 1' }), 'password');
  await user.type(screen.getByRole('textbox', { name: 'Environment variable name' }), 'INTERL_PASSWORD');
  await user.click(screen.getByRole('button', { name: 'Apply configuration' }));
  expect(useScenarioStore.getState().document!.identities[0].credentials).toEqual({
    password: { source: 'env', name: 'INTERL_PASSWORD' },
  });
  expect(useScenarioStore.getState().document!.workflows[0].nodes[1]).toEqual(auth);
});

it('keeps imported typed semantic errors editable without treating them as uncommitted fragments', async () => {
  const user = userEvent.setup();
  const document = request();
  const node = document.workflows[0].nodes[1];
  if (node.type !== 'http') throw new Error('HTTP');
  node.config.timeoutMs = 0;
  document.workflows[0].inputs.bad = { type: 'string', default: null };
  useScenarioStore.setState({ document, status: 'invalid' });
  render(<NodeInspector />);
  expect(useScenarioStore.getState().formErrors).toEqual({});
  await user.click(screen.getByRole('tab', { name: 'Workflow' }));
  expect(useScenarioStore.getState().formErrors).toEqual({});
  await user.clear(screen.getByRole('textbox', { name: 'Workflow name' }));
  await user.type(screen.getByRole('textbox', { name: 'Workflow name' }), 'Recoverable draft');
  await user.click(screen.getByRole('button', { name: 'Apply configuration' }));
  expect(useScenarioStore.getState().document!.workflows[0].inputs.bad.default).toBeNull();
});

it('retains invalid JSON when duplicate field names are corrected and removes only the intended row fragment', async () => {
  const user = userEvent.setup();
  function Editor() {
    const [value, setValue] = useState({ first: '', second: '' });
    return (
      <RecordFields
        label="Fields"
        value={value}
        onChange={(value) => setValue(value as typeof value & { first: string; second: string })}
        path="/fields"
        create={() => ''}
      >
        {(value, change, path) => (
          <JsonField label="Field JSON" value={value} path={path} onChange={(value) => change(value as string)} />
        )}
      </RecordFields>
    );
  }
  render(<Editor />);
  fireEvent.change(screen.getAllByRole('textbox', { name: 'Field JSON' })[0], { target: { value: '{broken' } });
  expect(screen.getAllByRole('textbox', { name: 'Field JSON' })[0]).toHaveValue('{broken');
  const name = screen.getByRole('textbox', { name: 'Fields name 2' });
  fireEvent.change(name, { target: { value: 'first' } });
  expect(screen.getAllByRole('textbox', { name: 'Field JSON' })[0]).toHaveValue('{broken');
  fireEvent.change(name, { target: { value: 'second' } });
  expect(Object.keys(useScenarioStore.getState().formErrors)).not.toHaveLength(0);
  expect(screen.getAllByRole('textbox', { name: 'Field JSON' })[0]).toHaveValue('{broken');
  await user.click(screen.getByRole('button', { name: 'Remove Fields row 1' }));
  expect(screen.getByRole('textbox', { name: 'Field JSON' })).toHaveValue('""');
  expect(useScenarioStore.getState().formErrors).toEqual({});
});

it('removes an invalid array item without transferring its fragment to an identical following item', async () => {
  const user = userEvent.setup();
  function Editor() {
    const [value, setValue] = useState<Template>({
      source: 'array',
      items: [
        { source: 'literal', value: '' },
        { source: 'literal', value: '' },
      ],
    });
    return (
      <SourceEditor
        value={value}
        onChange={(value) => setValue(value as Template)}
        path="/array"
        scope={{ document: request() }}
        template
      />
    );
  }
  render(<Editor />);
  fireEvent.change(screen.getAllByRole('textbox', { name: 'Literal JSON value' })[0], { target: { value: '{broken' } });
  await user.click(screen.getAllByRole('button', { name: 'Remove item' })[0]);
  expect(screen.getByRole('textbox', { name: 'Literal JSON value' })).toHaveValue('""');
  expect(useScenarioStore.getState().formErrors).toEqual({});
});
