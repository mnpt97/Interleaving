import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { executeCommand, newDraft } from './draft';
import { NodeIdentity } from './NodeIdentity';
import { createScenarioStore, useScenarioStore } from './store';
const initial = useScenarioStore.getState();
afterEach(() => useScenarioStore.setState(initial, true));
const workflowId = 'workflow-1';

it('renames every structured local reference while preserving literal payloads and other workflows', () => {
  let document = executeCommand(newDraft(), { type: 'http.insert', workflowId });
  document = executeCommand(document, { type: 'node.insert', workflowId, kind: 'extract' });
  document = executeCommand(document, { type: 'workflow.duplicate', workflowId });
  const workflow = document.workflows[0];
  workflow.outputs.status = { type: 'integer', value: { source: 'response', nodeId: 'http-1', field: 'status' } };
  workflow.nodes.push({
    id: 'template',
    type: 'http',
    config: {
      method: 'POST',
      path: '/',
      headers: { nodeId: 'http-1' },
      body: {
        kind: 'jsonTemplate',
        fields: {
          status: { source: 'array', items: [{ source: 'response', nodeId: 'http-1', field: 'status' }] },
          literal: { source: 'literal', value: { source: 'response', nodeId: 'http-1' } },
        },
      },
    },
  });
  workflow.nodes.push({ id: 'gateway', type: 'exclusiveSplit', config: { sourceNodeId: 'http-1', inputs: {} } });
  const next = executeCommand(document, { type: 'node.rename', workflowId, nodeId: 'http-1', nextId: 'health' });
  const renamed = next.workflows[0];
  expect(renamed.nodes.find((node) => node.id === 'extract-1')).toMatchObject({ config: { sourceNodeId: 'health' } });
  expect(renamed.nodes.find((node) => node.id === 'gateway')).toMatchObject({ config: { sourceNodeId: 'health' } });
  expect(renamed.outputs.status.value).toMatchObject({ nodeId: 'health' });
  expect(renamed.nodes.find((node) => node.id === 'template')).toMatchObject({
    config: {
      headers: { nodeId: 'http-1' },
      body: {
        fields: {
          status: { items: [{ nodeId: 'health' }] },
          literal: { value: { nodeId: 'http-1' } },
        },
      },
    },
  });
  expect(renamed.edges.some((edge) => edge.from === 'http-1' || edge.to === 'http-1')).toBe(false);
  expect(renamed.layout!.positions.health).toEqual(workflow.layout!.positions['http-1']);
  expect(renamed.layout!.positions['http-1']).toBeUndefined();
  expect(next.workflows[1]).toEqual(document.workflows[1]);
  expect(document.workflows[0].nodes.some((node) => node.id === 'http-1')).toBe(true);
});

it('rejects duplicate, empty, and oversized IDs without changing the document', () => {
  const document = newDraft();
  for (const nextId of ['end', '  ', 'é'.repeat(65)])
    expect(() => executeCommand(document, { type: 'node.rename', workflowId, nodeId: 'start', nextId })).toThrow();
  expect(document.workflows[0].nodes[0].id).toBe('start');
});

it('keeps the renamed node selected through undo and redo', () => {
  const store = createScenarioStore({ validate: vi.fn(), open: vi.fn(), save: vi.fn() });
  const document = newDraft();
  store.setState({
    document,
    mode: 'edit',
    selectedWorkflowId: workflowId,
    selectedId: 'start',
    formErrors: {},
    past: [],
    future: [],
  });
  store.getState().command({ type: 'node.rename', workflowId, nodeId: 'start', nextId: 'begin' });
  expect(store.getState().selectedId).toBe('begin');
  store.getState().undo();
  expect(store.getState().document).toEqual(document);
  expect(store.getState().selectedId).toBe('start');
  store.getState().redo();
  expect(store.getState().selectedId).toBe('begin');
});

it('stages ID changes and validates collisions before applying', () => {
  const document = newDraft();
  useScenarioStore.setState({
    document,
    mode: 'edit',
    selectedWorkflowId: workflowId,
    selectedId: 'start',
    formErrors: {},
  });
  render(<NodeIdentity workflow={document.workflows[0]} nodeId="start" />);
  fireEvent.change(screen.getByRole('textbox', { name: 'Node ID' }), { target: { value: 'end' } });
  expect(screen.getByRole('alert')).toHaveTextContent('already used');
  expect(screen.getByRole('button', { name: 'Apply configuration' })).toBeDisabled();
  fireEvent.change(screen.getByRole('textbox', { name: 'Node ID' }), { target: { value: 'begin' } });
  expect(useScenarioStore.getState().document).toBe(document);
  act(() => fireEvent.click(screen.getByRole('button', { name: 'Apply configuration' })));
  expect(useScenarioStore.getState().document!.workflows[0].nodes[0].id).toBe('begin');
});
