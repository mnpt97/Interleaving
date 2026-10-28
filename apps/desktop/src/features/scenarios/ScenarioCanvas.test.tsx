import { emptyEventContext } from '../../lib/run-types';
import { useState, type ComponentType, type CSSProperties, type ReactNode } from 'react';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import source from '../../../../../scenarios/examples/auth-login.interl.json?raw';
import { documentSchema } from '../../lib/scenario-types';
import { newDraft, executeCommand } from './draft';
import { ScenarioCanvas } from './ScenarioCanvas';
import { useScenarioStore } from './store';
import { useRunStore } from '../runs/store';
import type { ScenarioFlowNode } from './adapters';
import { ConnectionActions } from './ConnectionActions';

const updateNodeInternals = vi.hoisted(() => vi.fn());
beforeEach(() => useScenarioStore.setState({ canvasView: 'workflow' }));
vi.mock('../../lib/theme', () => ({ useTheme: () => ({ resolvedTheme: 'dark' }) }));
vi.mock('@xyflow/react', () => ({
  Position: { Left: 'left', Right: 'right', Top: 'top', Bottom: 'bottom' },
  ConnectionMode: { Strict: 'strict' },
  MarkerType: { ArrowClosed: 'arrowclosed' },
  EdgeToolbar: ({ children }: { children: ReactNode }) => <div data-testid="edge-toolbar">{children}</div>,
  NodeToolbar: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  Handle: ({ type, style }: { type: string; style?: CSSProperties }) => (
    <span data-testid={`${type}-handle`} style={style} />
  ),
  Background: () => null,
  Controls: () => null,
  ControlButton: () => null,
  applyNodeChanges: (_changes: unknown, nodes: unknown) => nodes,
  useUpdateNodeInternals: () => updateNodeInternals,
  useNodesInitialized: () => false,
  useReactFlow: () => ({ viewportInitialized: false, fitView: vi.fn() }),
  NodeResizeControl: () => null,
  useNodesState: (initial: ScenarioFlowNode[]) => useState(initial),
  // Exercise our real custom node and callbacks without simulating React Flow's DOM measurements.
  ReactFlow: ({
    nodes,
    nodeTypes,
    onNodeClick,
    nodesDraggable,
    connectionMode,
    snapToGrid,
    snapGrid,
    onNodesChange,
    children,
    onConnect,
    onReconnect,
    onEdgeClick,
    isValidConnection,
    edges,
  }: {
    children?: ReactNode;
    edges: { id: string; source: string; target: string; selected?: boolean }[];
    onConnect: (connection: { source: string; target: string; sourceHandle?: string; targetHandle?: string }) => void;
    onReconnect: (edge: { id: string }, connection: { source: string; target: string }) => void;
    onEdgeClick: (event: unknown, edge: { id: string }) => void;
    isValidConnection: (connection: { source: string; target: string }) => boolean;
    nodesDraggable: boolean;
    connectionMode: string;
    snapToGrid: boolean;
    snapGrid: [number, number];
    onNodesChange: (changes: unknown[]) => void;
    nodes: ScenarioFlowNode[];
    nodeTypes: { scenario: ComponentType<{ data: ScenarioFlowNode['data']; selected: boolean }> };
    onNodeClick: (event: unknown, node: ScenarioFlowNode) => void;
  }) => {
    const Node = nodeTypes.scenario;
    return (
      <div
        data-testid="canvas"
        data-draggable={nodesDraggable}
        data-connection-mode={connectionMode}
        data-snapping={snapToGrid}
        data-grid={snapGrid.join(',')}
      >
        {children}
        {edges.map((edge) => (
          <ConnectionActions key={edge.id} id={edge.id} selected={edge.selected} x={100} y={100} />
        ))}
        <button onClick={() => onConnect({ source: 'start', target: 'end' })}>Simulate connect</button>
        <button
          onClick={() => onConnect({ source: 'start', target: 'end', sourceHandle: 'top', targetHandle: 'bottom' })}
        >
          Connect corners
        </button>
        <button onClick={() => onReconnect({ id: edges[0]?.id ?? 'missing' }, { source: 'start', target: 'end' })}>
          Simulate reconnect
        </button>
        <button onClick={() => onEdgeClick(null, { id: edges[0]?.id ?? 'missing' })}>Select first connection</button>
        <span data-testid="connection-valid">{String(isValidConnection({ source: 'start', target: 'end' }))}</span>
        <button onClick={() => onNodesChange([{ type: 'position', id: 'health', position: { x: 999, y: 999 } }])}>
          Simulate drag
        </button>
        {nodes.map((node) => (
          <div key={node.id} data-testid={node.id} onClick={() => onNodeClick(null, node)}>
            <Node data={node.data} selected={Boolean(node.selected)} />
          </div>
        ))}
      </div>
    );
  },
}));
const scenarioInitial = useScenarioStore.getState();
const runInitial = useRunStore.getState();
afterEach(() => {
  cleanup();
  useScenarioStore.setState(scenarioInitial, true);
  useRunStore.setState(runInitial, true);
  updateNodeInternals.mockClear();
});

it('renders extract/session nodes, summaries and both execution handles', () => {
  useScenarioStore.setState({
    document: documentSchema.parse(JSON.parse(source)),
    selectedId: null,
    inspectorOpen: false,
  });
  render(<ScenarioCanvas />);
  const extract = screen.getByTestId('extract-token');
  expect(extract).toHaveTextContent('Extract');
  expect(extract).toHaveTextContent('login → token');
  expect(extract.querySelectorAll('[data-testid$="-handle"]')).toHaveLength(8);
  const session = screen.getByTestId('apply-token');
  expect(session).toHaveTextContent('Set session');
  expect(session).toHaveTextContent('1 set · 0 removed');
  expect(session.querySelectorAll('[data-testid$="-handle"]')).toHaveLength(8);
  fireEvent.click(session);
  expect(useScenarioStore.getState()).toMatchObject({ selectedId: 'apply-token', inspectorOpen: true });
});

it('keeps run progress separate from the current scenario canvas', () => {
  useScenarioStore.setState({ document: documentSchema.parse(JSON.parse(source)) });
  useRunStore.setState({
    scenarioId: 'auth-login',
    events: [
      {
        runId: 'run',
        sequence: 1,
        context: emptyEventContext,
        elapsedNs: 0,
        userId: null,
        nodeId: null,
        type: 'run.started',
        payload: {
          scenarioId: 'auth-login',
          graph: { groups: [], workflows: [{ key: 'wf:0', id: 'main', name: 'Main', nodes: [], edges: [] }] },
        },
      },
      {
        runId: 'run',
        sequence: 2,
        context: { ...emptyEventContext, workflowKey: 'wf:0' },
        elapsedNs: 1,
        userId: 'user:1',
        nodeId: 'extract-token',
        type: 'step.failed',
        payload: { error: { kind: 'missingProperty', message: 'Property is missing' } },
      },
    ],
  });
  const { unmount } = render(<ScenarioCanvas />);
  expect(screen.getByTestId('extract-token')).not.toHaveTextContent('Stopped');
  unmount();
  useRunStore.setState({ scenarioId: 'other-scenario' });
  render(<ScenarioCanvas />);
  expect(screen.getByTestId('extract-token')).not.toHaveTextContent('Stopped');
});

it('disables dragging and ignores position callbacks in View/Run, while Edit persists movement', async () => {
  render(<ScenarioCanvas />);
  const document = useScenarioStore.getState().document;
  expect(screen.getByTestId('canvas')).toHaveAttribute('data-draggable', 'false');
  expect(screen.getByTestId('canvas')).toHaveAttribute('data-snapping', 'false');
  for (const handle of screen.getAllByTestId(/-handle$/)) expect(handle).not.toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Simulate drag' }));
  expect(useScenarioStore.getState().document).toBe(document);
  expect(useScenarioStore.getState().dirty).toBe(false);
  await act(async () => {
    await useScenarioStore.getState().setMode('edit');
  });
  expect(screen.getByTestId('canvas')).toHaveAttribute('data-draggable', 'true');
  expect(screen.getByTestId('canvas')).toHaveAttribute('data-snapping', 'true');
  expect(screen.getByTestId('canvas')).toHaveAttribute('data-grid', '10,10');
  expect(screen.queryByRole('button', { name: 'Connect nodes' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Delete selected node' })).not.toBeInTheDocument();
  for (const handle of screen.getAllByTestId(/-handle$/)) expect(handle).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Simulate drag' }));
  expect(useScenarioStore.getState().document!.workflows[0].layout?.positions.health).toEqual({ x: 999, y: 999 });
});

it('routes real edge callbacks and keyboard deletion through undoable domain commands', () => {
  const document = executeCommand(newDraft(), { type: 'http.insert', workflowId: 'workflow-1' });
  useScenarioStore.setState({
    mode: 'edit',
    document,
    selectedWorkflowId: 'workflow-1',
    selectedId: null,
    formErrors: {},
    past: [],
    future: [],
  });
  render(<ScenarioCanvas />);
  expect(screen.queryByRole('button', { name: 'Delete connection' })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Select first connection' }));
  expect(screen.queryByRole('button', { name: 'Edit connection' })).not.toBeInTheDocument();
  expect(screen.getByTestId('edge-toolbar')).toContainElement(
    screen.getByRole('button', { name: 'Delete connection' })
  );
  fireEvent.click(screen.getByRole('button', { name: 'Delete connection' }));
  expect(useScenarioStore.getState().document!.workflows[0].edges).toHaveLength(1);
  act(() => useScenarioStore.getState().undo());
  expect(useScenarioStore.getState().document!.workflows[0].edges).toHaveLength(2);
  fireEvent.click(screen.getByTestId('http-1'));
  const editor = screen.getByRole('region', { name: 'Workflow editor' });
  const input = window.document.createElement('input');
  editor.append(input);
  fireEvent.keyDown(input, { key: 'Delete' });
  expect(useScenarioStore.getState().document!.workflows[0].nodes).toHaveLength(3);
  input.remove();
  fireEvent.keyDown(editor, { key: 'Delete' });
  expect(useScenarioStore.getState().document!.workflows[0].nodes).toHaveLength(2);
  expect(useScenarioStore.getState().document!.workflows[0].edges).toEqual([]);
  act(() => useScenarioStore.getState().undo());
  expect(useScenarioStore.getState().document).toEqual(document);
});

it('connects disconnected nodes, preserves reconnect IDs, and rejects callbacks in View', () => {
  const document = newDraft();
  document.workflows[0].edges = [];
  useScenarioStore.setState({
    mode: 'edit',
    document,
    selectedWorkflowId: 'workflow-1',
    selectedId: null,
    formErrors: {},
  });
  render(<ScenarioCanvas />);
  expect(screen.getByTestId('connection-valid')).toHaveTextContent('true');
  fireEvent.click(screen.getByRole('button', { name: 'Simulate connect' }));
  expect(useScenarioStore.getState().document!.workflows[0].edges).toEqual([
    { id: 'edge-1', from: 'start', to: 'end' },
  ]);
  fireEvent.click(screen.getByRole('button', { name: 'Simulate reconnect' }));
  expect(useScenarioStore.getState().document!.workflows[0].edges[0].id).toBe('edge-1');
  const before = useScenarioStore.getState().document;
  act(() => useScenarioStore.setState({ mode: 'view' }));
  expect(screen.getByTestId('connection-valid')).toHaveTextContent('false');
  fireEvent.click(screen.getByRole('button', { name: 'Simulate connect' }));
  fireEvent.click(screen.getByRole('button', { name: 'Simulate reconnect' }));
  fireEvent.keyDown(screen.getByRole('region', { name: 'Workflow editor' }), { key: 'Delete' });
  expect(useScenarioStore.getState().document).toBe(before);
  expect(screen.queryByRole('button', { name: 'Connect nodes' })).not.toBeInTheDocument();
});

it('previews a called workflow and returns to the caller without copying or dirtying it', () => {
  const document = executeCommand(newDraft(), {
    type: 'node.insert',
    workflowId: 'workflow-1',
    kind: 'call',
    newWorkflowName: 'Inventory',
  });
  useScenarioStore.setState({
    document,
    selectedWorkflowId: 'workflow-1',
    selectedId: null,
    workflowTrail: [],
    mode: 'view',
    dirty: false,
  });
  render(<ScenarioCanvas />);
  fireEvent.click(screen.getByText('Preview workflow'));
  fireEvent.click(screen.getByRole('button', { name: 'Open workflow' }));
  expect(useScenarioStore.getState().selectedWorkflowId).toBe('workflow-2');
  fireEvent.click(screen.getByRole('button', { name: 'Back to caller' }));
  expect(useScenarioStore.getState().selectedWorkflowId).toBe('workflow-1');
  expect(useScenarioStore.getState().selectedId).toBe('call-1');
  expect(useScenarioStore.getState().document).toBe(document);
  expect(useScenarioStore.getState().dirty).toBe(false);
});

it('replaces contextual Add/Delete with node icons and restores actions with Back', () => {
  useScenarioStore.setState({
    document: documentSchema.parse(JSON.parse(source)),
    mode: 'edit',
    selectedWorkflowId: 'main',
    selectedId: 'apply-token',
    formErrors: {},
  });
  render(<ScenarioCanvas />);
  expect(screen.getByRole('button', { name: 'Delete node' })).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Add after node' }));
  expect(screen.queryByRole('button', { name: 'Delete node' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Add after node' })).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'HTTP Request' })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Back' })).toHaveFocus();
  fireEvent.click(screen.getByRole('button', { name: 'Back' }));
  expect(screen.getByRole('button', { name: 'Delete node' })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Add after node' })).toHaveFocus();
});

it('renders exclusive splits and merges as diamonds with XOR markers', async () => {
  const { default: fixture } = await import('../../../../../scenarios/examples/exclusive-routing.interl.json?raw');
  useScenarioStore.setState({ document: documentSchema.parse(JSON.parse(fixture)), selectedWorkflowId: 'main' });
  render(<ScenarioCanvas />);
  for (const id of ['result', 'joined']) {
    const node = screen.getByTestId(id);
    expect(node.querySelector('.scenario-node-gateway')).not.toBeNull();
    expect(node.querySelector('[aria-label="Exclusive gateway"]')).not.toBeNull();
    expect(node.querySelectorAll('[data-testid="source-handle"]')).toHaveLength(4);
    expect(node.querySelectorAll('[data-testid="target-handle"]')).toHaveLength(4);
  }
});

it('changes a node using the contextual icon picker and retains selection', () => {
  const document = executeCommand(newDraft(), { type: 'http.insert', workflowId: 'workflow-1' });
  useScenarioStore.setState({
    document,
    mode: 'edit',
    selectedWorkflowId: 'workflow-1',
    selectedId: 'http-1',
    formErrors: {},
    error: null,
  });
  render(<ScenarioCanvas />);
  fireEvent.click(screen.getByRole('button', { name: 'Change node type' }));
  expect(screen.queryByRole('button', { name: 'Delete node' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Add after node' })).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'HTTP Request' })).toBeDisabled();
  updateNodeInternals.mockClear();
  fireEvent.click(screen.getByRole('button', { name: 'Authentication' }));
  expect(updateNodeInternals).toHaveBeenCalledWith('http-1');
  expect(useScenarioStore.getState().document!.workflows[0].nodes.find((node) => node.id === 'http-1')?.type).toBe(
    'authentication'
  );
  expect(useScenarioStore.getState().selectedId).toBe('http-1');
  expect(screen.getByRole('button', { name: 'Delete node' })).toBeInTheDocument();
});

it('connects using chosen sides with explicit inputs and outputs and persists attachment layout', () => {
  const document = newDraft();
  document.workflows[0].edges = [];
  useScenarioStore.setState({
    document,
    mode: 'edit',
    selectedWorkflowId: 'workflow-1',
    selectedId: null,
    formErrors: {},
  });
  render(<ScenarioCanvas />);
  expect(screen.getByTestId('canvas')).toHaveAttribute('data-connection-mode', 'strict');
  fireEvent.click(screen.getByRole('button', { name: 'Connect corners' }));
  expect(useScenarioStore.getState().document!.workflows[0].layout?.connections?.['edge-1']).toEqual({
    source: 'top',
    target: 'bottom',
  });
});
