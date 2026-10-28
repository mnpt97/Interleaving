import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import example from '../../../../../scenarios/examples/exclusive-routing.interl.json?raw';
import { documentSchema } from '../../lib/scenario-types';
import { executeCommand, newDraft } from './draft';
import { useScenarioStore } from './store';
import { GatewayConfiguration } from './GatewayConfiguration';
import { AddNodePopover } from './AddNodePopover';
import { earlierNodes } from './ConfigurationFields';
import { connectionError } from './graph';
const initial = useScenarioStore.getState();
afterEach(() => useScenarioStore.setState(initial, true));
const load = () => documentSchema.parse(JSON.parse(example));

it('inserts a paired region atomically and preserves route metadata when inserting arm nodes', () => {
  let document = executeCommand(newDraft(), { type: 'http.insert', workflowId: 'workflow-1' });
  document = executeCommand(document, {
    type: 'node.insert',
    workflowId: 'workflow-1',
    kind: 'exclusiveSplit',
    afterNodeId: 'http-1',
  });
  const workflow = document.workflows[0];
  const split = workflow.nodes.find((node) => node.type === 'exclusiveSplit')!;
  expect(split.type).toBe('exclusiveSplit');
  if (split.type !== 'exclusiveSplit') return;
  expect(split.config.sourceNodeId).toBe('http-1');
  expect(workflow.nodes.find((node) => node.type === 'exclusiveMerge')?.type).toBe('exclusiveMerge');
  const route = workflow.edges.find((edge) => edge.from === split.id && edge.condition)!;
  const withArm = executeCommand(document, {
    type: 'node.insert',
    workflowId: workflow.id,
    kind: 'http',
    edgeId: route.id,
  });
  expect(withArm.workflows[0].edges.find((edge) => edge.id === route.id)).toMatchObject({
    from: split.id,
    condition: route.condition,
  });
  expect(withArm.workflows[0].edges.find((edge) => edge.id === route.id)?.to).not.toBe(route.to);
  expect(document.workflows[0]).toEqual(workflow);
});

it('creates a single disconnected gateway from the actual palette and undo restores the original graph', async () => {
  const document = executeCommand(newDraft(), { type: 'http.insert', workflowId: 'workflow-1' });
  useScenarioStore.setState({
    document,
    mode: 'edit',
    selectedWorkflowId: 'workflow-1',
    selectedId: 'http-1',
    past: [],
    formErrors: {},
  });
  render(<AddNodePopover afterNodeId="http-1" />);
  fireEvent.click(screen.getByRole('button', { name: 'Add after node' }));
  fireEvent.click(screen.getByRole('button', { name: 'Gateway' }));
  expect(screen.getByRole('button', { name: 'Parallel' })).toBeDisabled();
  fireEvent.click(screen.getByRole('button', { name: 'Exclusive' }));
  expect(useScenarioStore.getState().document!.workflows[0].nodes.some((node) => node.type === 'exclusiveSplit')).toBe(
    true
  );
  expect(useScenarioStore.getState().document!.workflows[0].edges).toEqual(document.workflows[0].edges);
  expect(useScenarioStore.getState().document!.workflows[0].nodes).toHaveLength(document.workflows[0].nodes.length + 1);
  act(() => useScenarioStore.getState().undo());
  expect(useScenarioStore.getState().document).toEqual(document);
  act(() => useScenarioStore.getState().redo());
  expect(useScenarioStore.getState().document!.workflows[0].nodes.some((node) => node.type === 'exclusiveMerge')).toBe(
    false
  );
});

it('stages predicate edits and applies them as one command; View only describes routes', () => {
  const document = load();
  const workflow = document.workflows[0];
  const node = workflow.nodes.find((node) => node.type === 'exclusiveSplit')!;
  if (node.type !== 'exclusiveSplit') throw new Error('fixture');
  useScenarioStore.setState({ document, mode: 'edit', selectedWorkflowId: workflow.id, formErrors: {} });
  const { unmount } = render(
    <GatewayConfiguration node={node} workflow={workflow} editing path="/workflows/0/nodes/2" />
  );
  expect(screen.queryByRole('combobox', { name: 'Paired merge' })).not.toBeInTheDocument();
  expect(screen.queryByRole('combobox', { name: 'Paired split' })).not.toBeInTheDocument();
  fireEvent.change(screen.getByRole('textbox', { name: 'Condition success' }), {
    target: { value: 'response.status = 201' },
  });
  expect(useScenarioStore.getState().document).toBe(document);
  fireEvent.click(screen.getByRole('button', { name: 'Apply configuration' }));
  expect(useScenarioStore.getState().document!.workflows[0].edges[2].condition?.expression).toBe(
    'response.status = 201'
  );
  unmount();
  render(<GatewayConfiguration node={node} workflow={workflow} editing={false} path="/workflows/0/nodes/2" />);
  expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
  expect(screen.getByText(/Response source: health/)).toBeInTheDocument();
});

it('allows gateway reconnection and offers only all-path sources after a merge', () => {
  const document = load();
  const workflow = document.workflows[0];
  expect(connectionError(workflow, 'result', 'joined')).toBeNull();
  expect(earlierNodes({ document, workflow, nodeId: 'joined' }).map((node) => node.id)).toContain('health');
  expect(earlierNodes({ document, workflow, nodeId: 'joined' }).map((node) => node.id)).not.toContain('accepted');
  expect(earlierNodes({ document, workflow }).map((node) => node.id)).not.toContain('fallback');
});

it('opens route diagnostics at the split inspector', () => {
  const document = load();
  useScenarioStore.setState({ document, formErrors: {} });
  act(() =>
    useScenarioStore.getState().inspectDiagnostic({
      category: 'validation',
      code: 'invalidRouteCondition',
      nodeId: null,
      filePath: null,
      line: null,
      column: null,
      fieldPath: '/workflows/0/edges/2/condition/expression',
      message: 'Invalid route',
      workflowId: 'main',
    })
  );
  expect(useScenarioStore.getState()).toMatchObject({
    selectedWorkflowId: 'main',
    selectedId: 'result',
    inspectorTab: 'node',
    focusedFieldPath: '/workflows/0/edges/2/condition/expression',
  });
});

it('reproduces the executable branching fixture through editor commands', async () => {
  const { default: fixture } =
    await import('../../../../../crates/interl-engine/tests/fixtures/editor-routing.json?raw');
  const { moveNode } = await import('./adapters');
  let document = newDraft();
  document.id = 'editor-routing';
  document.name = 'Editor routing';
  const workflowId = document.workflows[0].id;
  const insert = (kind: 'http' | 'exclusiveSplit', edgeId?: string, afterNodeId?: string) => {
    document = executeCommand(document, { type: 'node.insert', workflowId, kind, edgeId, afterNodeId });
  };
  const configure = (id: string, path: string) => {
    const node = document.workflows[0].nodes.find((node) => node.id === id)!;
    if (node.type !== 'http') throw new Error('expected request');
    document = executeCommand(document, {
      type: 'node.put',
      workflowId,
      node: { ...node, config: { ...node.config, path } },
    });
  };
  insert('http');
  configure('http-1', '/health');
  insert('exclusiveSplit', undefined, 'http-1');
  insert('http', 'edge-3');
  configure('http-2', '/echo');
  insert('http', 'edge-4');
  configure('http-3', '/health');
  document = moveNode(document, 'http-2', { x: 860, y: 60 });
  document = moveNode(document, 'http-3', { x: 860, y: 260 });
  expect(document).toEqual(documentSchema.parse(JSON.parse(fixture)));
});
