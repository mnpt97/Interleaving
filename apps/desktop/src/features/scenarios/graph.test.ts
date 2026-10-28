import linearFixture from '../../../../../crates/interl-engine/tests/fixtures/editor-linear.json?raw';
import { describe, expect, it, vi } from 'vitest';
import { executeCommand, newDraft } from './draft';
import { canCall, connectionError } from './graph';
import { toFlowEdges } from './adapters';
import { createScenarioStore } from './store';

const workflowId = 'workflow-1';
function inserted() {
  return executeCommand(newDraft(), { type: 'http.insert', workflowId });
}
function store() {
  const store = createScenarioStore({ validate: vi.fn(), open: vi.fn(), save: vi.fn() });
  const document = inserted();
  store.setState({
    mode: 'edit',
    document,
    selectedWorkflowId: workflowId,
    savedDocument: JSON.stringify(document),
    selectedId: 'http-1',
  });
  return store;
}

describe('directed graph commands', () => {
  it('rejects invalid endpoints, duplicate edges, self-links, and cycles', () => {
    const draft = inserted();
    const workflow = draft.workflows[0];
    expect(connectionError(workflow, 'start', 'http-1')).toContain('already exists');
    expect(connectionError(workflow, 'http-1', 'http-1')).toContain('itself');
    expect(connectionError(workflow, 'end', 'http-1')).toContain('End');
    expect(connectionError(workflow, 'http-1', 'start')).toContain('Start');
    expect(connectionError(workflow, 'missing', 'end')).toContain('current workflow');
    expect(connectionError(workflow, 'start', 'end')).toBeNull();
    workflow.nodes.push({ id: 'other', type: 'setSession', config: { setHeaders: {}, removeHeaders: [] } });
    expect(connectionError(workflow, 'other', 'http-1')).toBeNull();
    workflow.edges = [{ id: 'a', from: 'http-1', to: 'other' }];
    expect(connectionError(workflow, 'other', 'http-1')).toContain('cycle');
  });

  it('supports delete, connect, and reconnect as distinct undoable edits without changing edge identity', () => {
    const state = store();
    state.getState().command({ type: 'graph.delete', workflowId, nodeIds: [], edgeIds: ['edge-1'] });
    expect(state.getState().status).toBe('unchecked');
    state.getState().command({ type: 'edge.connect', workflowId, from: 'start', to: 'http-1' });
    expect(state.getState().document!.workflows[0].edges).toHaveLength(2);
    state.getState().undo();
    expect(state.getState().document!.workflows[0].edges).toHaveLength(1);
    state.getState().redo();
    const edge = state.getState().document!.workflows[0].edges.find((edge) => edge.from === 'start')!;
    state.getState().command({ type: 'edge.reconnect', workflowId, edgeId: edge.id, from: 'start', to: 'end' });
    expect(state.getState().error).toBeNull();
    expect(state.getState().document!.workflows[0].edges.find((item) => item.id === edge.id)).toEqual({
      ...edge,
      to: 'end',
    });
    state.getState().command({ type: 'graph.delete', workflowId, nodeIds: [], edgeIds: ['edge-2'] });
    state.getState().command({ type: 'edge.reconnect', workflowId, edgeId: edge.id, from: 'start', to: 'end' });
    expect(state.getState().document!.workflows[0].edges).toEqual([{ ...edge, to: 'end' }]);
  });

  it('deletes a node and all attached edges atomically and restores its layout on undo', () => {
    const state = store();
    const original = structuredClone(state.getState().document);
    state.getState().command({ type: 'graph.delete', workflowId, nodeIds: ['http-1'], edgeIds: [] });
    expect(state.getState().document!.workflows[0].edges).toEqual([]);
    expect(state.getState().document!.workflows[0].layout!.positions['http-1']).toBeUndefined();
    expect(state.getState().selectedId).toBeNull();
    state.getState().undo();
    expect(state.getState().document).toEqual(original);
  });

  it('protects boundaries and external consumers while permitting consumer/source batch deletion', () => {
    let draft = executeCommand(inserted(), { type: 'node.insert', workflowId, kind: 'extract' });
    expect(() => executeCommand(draft, { type: 'node.delete', workflowId, nodeId: 'start' })).toThrow('boundary');
    expect(() => executeCommand(draft, { type: 'node.delete', workflowId, nodeId: 'http-1' })).toThrow('extract-1');
    draft.workflows[0].outputs.value = {
      type: 'string',
      value: { source: 'extraction', nodeId: 'extract-1', name: 'token' },
    };
    expect(() =>
      executeCommand(draft, { type: 'graph.delete', workflowId, nodeIds: ['http-1', 'extract-1'], edgeIds: [] })
    ).toThrow('workflow output value');
    delete draft.workflows[0].outputs.value;
    const deleted = executeCommand(draft, {
      type: 'graph.delete',
      workflowId,
      nodeIds: ['http-1', 'extract-1'],
      edgeIds: [],
    });
    expect(deleted.workflows[0].nodes.map((node) => node.type)).toEqual(['start', 'end']);
  });

  it('appends to a disconnected output and makes room along connectivity rather than array order', () => {
    let draft = inserted();
    draft = executeCommand(draft, { type: 'node.insert', workflowId, kind: 'setSession', afterNodeId: 'start' });
    expect(draft.workflows[0].layout!.positions['http-1'].x).toBe(600);
    expect(draft.workflows[0].layout!.positions.end.x).toBe(860);
    expect(draft.workflows[0].edges).toContainEqual(expect.objectContaining({ from: 'start', to: 'setSession-1' }));
    draft.workflows[0].edges = [];
    draft = executeCommand(draft, { type: 'node.insert', workflowId, kind: 'http', afterNodeId: 'http-1' });
    expect(draft.workflows[0].edges).toEqual([{ id: 'edge-1', from: 'http-1', to: 'http-2' }]);
  });

  it('creates a reusable workflow and its call atomically, rejects recursion, and recovers navigation after undo', () => {
    const state = store();
    state.getState().command({ type: 'node.insert', workflowId, kind: 'call', newWorkflowName: 'Subprocess' });
    const draft = state.getState().document!;
    expect(draft.workflows).toHaveLength(2);
    expect(draft.workflows[1].name).toBe('Subprocess');
    expect(canCall(draft, 'workflow-2', workflowId)).toBe(false);
    expect(canCall(draft, workflowId, workflowId)).toBe(false);
    state.getState().enterWorkflow('call-1');
    expect(state.getState().selectedWorkflowId).toBe('workflow-2');
    state.getState().leaveWorkflow();
    expect(state.getState().selectedId).toBe('call-1');
    state.getState().enterWorkflow('call-1');
    state.getState().undo();
    expect(state.getState().selectedWorkflowId).toBe(workflowId);
    expect(state.getState().workflowTrail).toEqual([]);
  });

  it('never mutates a View document through graph commands and scopes same-named nodes to their workflow', () => {
    const state = store();
    state.setState({ mode: 'view' });
    const before = state.getState().document;
    state.getState().command({ type: 'graph.delete', workflowId, nodeIds: ['http-1'], edgeIds: [] });
    expect(state.getState().document).toBe(before);
    let draft = executeCommand(before!, { type: 'workflow.duplicate', workflowId });
    draft = executeCommand(draft, { type: 'node.delete', workflowId: 'workflow-2', nodeId: 'http-1' });
    expect(draft.workflows[0]).toEqual(before!.workflows[0]);
  });

  it('projects arrowheads, accessible edge names, and large selection targets without persisting renderer metadata', () => {
    const draft = inserted();
    const edge = toFlowEdges(draft)[0];
    expect(edge.markerEnd).toMatchObject({ type: 'arrowclosed' });
    expect(edge.interactionWidth).toBeGreaterThanOrEqual(20);
    expect(edge.ariaLabel).toContain('start to http-1');
    expect(JSON.stringify(draft)).not.toMatch(/markerEnd|interactionWidth/);
  });
});

it('does not mistake literal body or port data for executable references during deletion', () => {
  const draft = inserted();
  draft.workflows[0].nodes.push({
    id: 'payload',
    type: 'http',
    config: { method: 'POST', path: '/', headers: {}, body: { kind: 'json', value: { nodeId: 'http-1' } } },
  });
  draft.workflows[0].outputs.literal = { type: 'string', value: { source: 'literal', value: { nodeId: 'http-1' } } };
  expect(() => executeCommand(draft, { type: 'node.delete', workflowId, nodeId: 'http-1' })).not.toThrow();
});

it('constructs the Rust-validated linear fixture from New using insertion and inspector commands', () => {
  let document = newDraft();
  document.id = 'editor-linear';
  document = executeCommand(document, { type: 'name', name: 'Editor linear workflow' });
  document = executeCommand(document, { type: 'http.insert', workflowId });
  const first = document.workflows[0].nodes.find((node) => node.id === 'http-1');
  if (first?.type !== 'http') throw new Error('Expected HTTP');
  document = executeCommand(document, {
    type: 'node.put',
    workflowId,
    node: { ...first, config: { ...first.config, path: '/health' } },
  });
  document = executeCommand(document, { type: 'node.insert', workflowId, kind: 'http', afterNodeId: 'http-1' });
  const second = document.workflows[0].nodes.find((node) => node.id === 'http-2');
  if (second?.type !== 'http') throw new Error('Expected HTTP');
  document = executeCommand(document, {
    type: 'node.put',
    workflowId,
    node: { ...second, config: { ...second.config, path: '/echo' } },
  });
  expect(document).toEqual(JSON.parse(linearFixture));
});

it('changes a node type without changing its ID, connections, or layout and supports undo/redo', () => {
  const state = store();
  const before = state.getState().document!;
  state.getState().command({ type: 'node.change', workflowId, nodeId: 'http-1', kind: 'authentication' });
  const after = state.getState().document!;
  expect(after.workflows[0].nodes.find((node) => node.id === 'http-1')?.type).toBe('authentication');
  expect(after.workflows[0].edges).toEqual(before.workflows[0].edges);
  expect(after.workflows[0].layout).toEqual(before.workflows[0].layout);
  state.getState().undo();
  expect(state.getState().document).toEqual(before);
  state.getState().redo();
  expect(state.getState().document).toEqual(after);
});

it('protects referenced producers and workflow boundaries during type changes', () => {
  const document = executeCommand(inserted(), { type: 'node.insert', workflowId, kind: 'extract' });
  expect(() =>
    executeCommand(document, { type: 'node.change', workflowId, nodeId: 'http-1', kind: 'authentication' })
  ).toThrow('output references');
  expect(() => executeCommand(document, { type: 'node.change', workflowId, nodeId: 'start', kind: 'http' })).toThrow(
    'cannot change type'
  );
});

it('uses a previous HTTP source when changing to Extract and changes to a single gateway and back without automatic wiring', () => {
  const document = executeCommand(inserted(), { type: 'node.insert', workflowId, kind: 'http' });
  const extract = executeCommand(document, { type: 'node.change', workflowId, nodeId: 'http-2', kind: 'extract' });
  expect(extract.workflows[0].nodes.find((node) => node.id === 'http-2')).toMatchObject({
    type: 'extract',
    config: { sourceNodeId: 'http-1' },
  });
  const branched = executeCommand(document, {
    type: 'node.change',
    workflowId,
    nodeId: 'http-2',
    kind: 'exclusiveSplit',
  });
  const workflow = branched.workflows[0];
  const split = workflow.nodes.find((node) => node.id === 'http-2');
  expect(split).toMatchObject({ type: 'exclusiveSplit', config: { sourceNodeId: 'http-1', inputs: {} } });
  expect(workflow.nodes).toHaveLength(document.workflows[0].nodes.length);
  expect(workflow.edges).toEqual(
    document.workflows[0].edges.map((edge) =>
      edge.from === 'http-2' ? { ...edge, condition: { expression: 'false' } } : edge
    )
  );
  const restored = executeCommand(branched, { type: 'node.change', workflowId, nodeId: 'http-2', kind: 'http' });
  expect(restored.workflows[0].nodes.find((node) => node.id === 'http-2')?.type).toBe('http');
  expect(restored.workflows[0].edges).toEqual(document.workflows[0].edges);
  expect(workflow.layout?.positions['http-2']).toEqual(document.workflows[0].layout?.positions['http-2']);
});

it('places each new node independently without changing existing edges or positions', () => {
  const before = inserted();
  const added = executeCommand(before, { type: 'node.add', workflowId, kind: 'exclusiveSplit', afterNodeId: 'http-1' });
  expect(added.workflows[0].nodes).toHaveLength(before.workflows[0].nodes.length + 1);
  expect(added.workflows[0].edges).toEqual(before.workflows[0].edges);
  expect(added.workflows[0].nodes.filter((node) => node.type === 'exclusiveMerge')).toEqual([]);
  for (const [id, position] of Object.entries(before.workflows[0].layout!.positions))
    expect(added.workflows[0].layout?.positions[id]).toEqual(position);
  const restored = executeCommand(added, { type: 'node.change', workflowId, nodeId: 'exclusiveSplit-1', kind: 'http' });
  expect(restored.workflows[0].nodes.find((node) => node.id === 'exclusiveSplit-1')?.type).toBe('http');
  expect(restored.workflows[0].edges).toEqual(before.workflows[0].edges);
});

it('stores attachment sides in layout and retains them through movement and reconnection', async () => {
  const { moveNode } = await import('./adapters');
  let document = newDraft();
  document.workflows[0].edges = [];
  document = executeCommand(document, {
    type: 'edge.connect',
    workflowId,
    from: 'start',
    to: 'end',
    sourceHandle: 'top',
    targetHandle: 'bottom',
  });
  expect(document.workflows[0].layout?.connections?.['edge-1']).toEqual({ source: 'top', target: 'bottom' });
  expect(toFlowEdges(document)[0]).toMatchObject({ sourceHandle: 'top', targetHandle: 'bottom' });
  document = moveNode(document, 'start', { x: 10, y: 20 });
  expect(document.workflows[0].layout?.connections?.['edge-1']).toEqual({ source: 'top', target: 'bottom' });
  document = executeCommand(document, {
    type: 'edge.reconnect',
    workflowId,
    edgeId: 'edge-1',
    from: 'start',
    to: 'end',
    sourceHandle: 'left',
    targetHandle: 'right',
  });
  expect(toFlowEdges(document)[0]).toMatchObject({ id: 'edge-1', sourceHandle: 'left', targetHandle: 'right' });
  document = executeCommand(document, { type: 'graph.delete', workflowId, nodeIds: [], edgeIds: ['edge-1'] });
  expect(document.workflows[0].layout?.connections).toEqual({});
});

it('keeps the merge when a split changes type, without persisted pair configuration', () => {
  let document = executeCommand(inserted(), { type: 'node.add', workflowId, kind: 'exclusiveSplit' });
  document = executeCommand(document, { type: 'node.add', workflowId, kind: 'exclusiveMerge' });
  const changed = executeCommand(document, {
    type: 'node.change',
    workflowId,
    nodeId: 'exclusiveSplit-1',
    kind: 'http',
  });
  expect(changed.workflows[0].nodes).toHaveLength(document.workflows[0].nodes.length);
  expect(changed.workflows[0].nodes.find((node) => node.id === 'exclusiveMerge-1')).toMatchObject({
    config: {},
  });
});
