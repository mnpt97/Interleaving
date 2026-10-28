import { act, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { AddNodePopover } from './AddNodePopover';
import { NodeInspector } from './NodeInspector';
import { executeCommand, newDraft } from './draft';
import { useScenarioStore } from './store';
const initial = useScenarioStore.getState();
afterEach(() => useScenarioStore.setState(initial, true));
const workflowId = 'workflow-1';

it('creates a reusable workflow through the palette without reintroducing workflow management buttons', async () => {
  const user = userEvent.setup();
  useScenarioStore.setState({ mode: 'edit', document: newDraft(), selectedWorkflowId: workflowId, selectedId: null });
  render(<AddNodePopover />);
  await user.click(screen.getByRole('button', { name: 'Add node' }));
  await user.click(screen.getByRole('button', { name: 'Call workflow' }));
  const name = screen.getByRole('textbox', { name: 'Workflow name' });
  await user.clear(name);
  await user.type(name, 'Read account');
  await user.click(screen.getByRole('button', { name: 'Insert workflow call' }));
  const state = useScenarioStore.getState();
  expect(state.document!.workflows[1].name).toBe('Read account');
  expect(state.document!.workflows[0].nodes.find((node) => node.id === 'call-1')).toMatchObject({
    type: 'call',
    config: { workflowId: 'workflow-2' },
  });
  expect(state.selectedId).toBe('call-1');
  act(() => state.undo());
  expect(useScenarioStore.getState().document!.workflows).toHaveLength(1);
});

it('offers contextual insertion on a specific output and leaves unrelated workflows unchanged', async () => {
  const user = userEvent.setup();
  const document = executeCommand(newDraft(), { type: 'workflow.duplicate', workflowId });
  useScenarioStore.setState({ mode: 'edit', document, selectedWorkflowId: workflowId, selectedId: 'start' });
  render(<AddNodePopover afterNodeId="start" />);
  await user.click(screen.getByRole('button', { name: 'Add after node' }));
  await user.click(screen.getByRole('button', { name: /HTTP Request/ }));
  expect(useScenarioStore.getState().document!.workflows[0].nodes.find((node) => node.id === 'http-1')?.type).toBe(
    'http'
  );
  expect(useScenarioStore.getState().document!.workflows[1]).toEqual(document.workflows[1]);
});

it('edits an extraction as a staged typed command and never evaluates its expression in React', async () => {
  const user = userEvent.setup();
  const http = executeCommand(newDraft(), { type: 'http.insert', workflowId });
  const document = executeCommand(http, { type: 'node.insert', workflowId, kind: 'extract' });
  useScenarioStore.setState({ mode: 'edit', document, selectedWorkflowId: workflowId, selectedId: 'extract-1' });
  render(<NodeInspector />);
  const expression = screen.getByRole('textbox', { name: 'Property expression' });
  await user.clear(expression);
  await user.type(expression, 'response.body.accessToken');
  expect(Object.keys(useScenarioStore.getState().formErrors)).not.toHaveLength(0);
  await user.click(screen.getByRole('button', { name: 'Apply extraction' }));
  expect(
    useScenarioStore.getState().document!.workflows[0].nodes.find((node) => node.id === 'extract-1')
  ).toMatchObject({
    config: { outputs: { token: { expression: 'response.body.accessToken', type: 'string' } }, sourceNodeId: 'http-1' },
  });
  expect(useScenarioStore.getState().formErrors).toEqual({});
});

it('stages multiple named typed extraction outputs and discards incomplete rows', async () => {
  const user = userEvent.setup();
  const document = executeCommand(executeCommand(newDraft(), { type: 'http.insert', workflowId }), {
    type: 'node.insert',
    workflowId,
    kind: 'extract',
  });
  useScenarioStore.setState({ mode: 'edit', document, selectedWorkflowId: workflowId, selectedId: 'extract-1' });
  render(<NodeInspector />);
  await user.click(screen.getByRole('button', { name: 'Add outputs' }));
  expect(screen.getByRole('button', { name: 'Apply extraction' })).toBeDisabled();
  await user.click(screen.getByRole('button', { name: 'Discard fields' }));
  expect(screen.getAllByRole('textbox', { name: 'Property expression' })).toHaveLength(1);
  for (const [name, type] of [
    ['ready', 'boolean'],
    ['attempts', 'integer'],
  ]) {
    await user.click(screen.getByRole('button', { name: 'Add outputs' }));
    const names = screen.getAllByRole('textbox', { name: /Outputs name/ });
    await user.type(names[names.length - 1], name);
    const expressions = screen.getAllByRole('textbox', { name: 'Property expression' });
    await user.clear(expressions[expressions.length - 1]);
    await user.type(expressions[expressions.length - 1], `response.body.value.${name}`);
    const types = screen.getAllByRole('combobox', { name: 'Output type' });
    await user.selectOptions(types[types.length - 1], type);
  }
  expect(screen.getByRole('button', { name: 'Apply extraction' })).toBeEnabled();
  await user.click(screen.getByRole('button', { name: 'Apply extraction' }));
  expect(useScenarioStore.getState().error).toBeNull();
  expect(
    useScenarioStore.getState().document!.workflows[0].nodes.find((node) => node.id === 'extract-1')
  ).toMatchObject({
    config: { outputs: { token: { type: 'string' }, ready: { type: 'boolean' }, attempts: { type: 'integer' } } },
  });
  expect(useScenarioStore.getState().formErrors).toEqual({});
});

it('protects extraction outputs that have downstream mappings', () => {
  const document = executeCommand(executeCommand(newDraft(), { type: 'http.insert', workflowId }), {
    type: 'node.insert',
    workflowId,
    kind: 'extract',
  });
  const workflow = document.workflows[0];
  workflow.outputs.token = { type: 'string', value: { source: 'extraction', nodeId: 'extract-1', name: 'token' } };
  const node = workflow.nodes.find((node) => node.id === 'extract-1')!;
  if (node.type !== 'extract') throw new Error('fixture');
  expect(() =>
    executeCommand(document, {
      type: 'node.put',
      workflowId,
      node: {
        ...node,
        config: { sourceNodeId: 'http-1', outputs: { renamed: { expression: 'response.body.token', type: 'string' } } },
      },
    })
  ).toThrow(/referenced/);
});

it('configures reference-based session operations and removes them undoably', async () => {
  const user = userEvent.setup();
  const http = executeCommand(newDraft(), { type: 'http.insert', workflowId });
  const extract = executeCommand(http, { type: 'node.insert', workflowId, kind: 'extract' });
  const document = executeCommand(extract, { type: 'node.insert', workflowId, kind: 'setSession' });
  useScenarioStore.setState({ mode: 'edit', document, selectedWorkflowId: workflowId, selectedId: 'setSession-1' });
  render(<NodeInspector />);
  await user.selectOptions(screen.getByRole('combobox', { name: 'Value source' }), 'extraction');
  await user.selectOptions(
    screen.getByRole('combobox', { name: 'Extract output' }),
    screen.getByRole('option', { name: 'extract-1.token' })
  );
  await user.click(screen.getByRole('button', { name: 'Apply header operation' }));
  expect(
    useScenarioStore.getState().document!.workflows[0].nodes.find((node) => node.id === 'setSession-1')
  ).toMatchObject({
    config: {
      setHeaders: { Authorization: { source: 'extraction', nodeId: 'extract-1', name: 'token', prefix: 'Bearer ' } },
    },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Remove set operation Authorization' }));
  expect(screen.queryByText('Set Authorization · extraction')).not.toBeInTheDocument();
  act(() => useScenarioStore.getState().undo());
  expect(screen.getByText('Set Authorization · extraction')).toBeInTheDocument();
});
