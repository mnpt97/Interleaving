import { scopedId } from '../scenarios/lifecycle';
import userEvent from '@testing-library/user-event';
import type { ComponentType, ReactNode } from 'react';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { RunCanvas } from './RunCanvas';
import { RunSidebar } from './RunSidebar';
import { RunInspector } from './RunInspector';
import { useRunStore } from './store';
import { useScenarioStore } from '../scenarios/store';
import { begin, completeReport, observation as event, response, runGraph } from './fixtures';
import { RunPanel } from './RunPanel';

vi.mock('../../lib/theme', () => ({ useTheme: () => ({ resolvedTheme: 'dark' }) }));
vi.mock('@xyflow/react', () => ({
  Position: { Left: 'left', Right: 'right', Top: 'top', Bottom: 'bottom' },
  MarkerType: { ArrowClosed: 'arrow' },
  Background: () => null,
  Controls: () => null,
  ControlButton: () => null,
  Handle: () => null,
  useNodesInitialized: () => false,
  useReactFlow: () => ({ viewportInitialized: false, fitView: vi.fn() }),
  ReactFlow: ({
    nodes,
    edges,
    nodeTypes,
    nodesDraggable,
    nodesConnectable,
    onNodeClick,
    onNodeDoubleClick,
    children,
  }: {
    nodes: { id: string; type: string; data: Record<string, unknown>; position: { x: number; y: number } }[];
    edges: { id: string; sourceHandle: string; targetHandle: string; className: string; data: { motion: boolean } }[];
    nodeTypes: Record<string, ComponentType<{ data: never; selected: boolean }>>;
    nodesDraggable: boolean;
    nodesConnectable: boolean;
    onNodeClick: (event: null, node: { id: string }) => void;
    onNodeDoubleClick: (event: null, node: { id: string }) => void;
    children: ReactNode;
  }) => {
    return (
      <div data-testid="run-canvas" data-draggable={nodesDraggable} data-connectable={nodesConnectable}>
        {children}
        {nodes.map((node) => {
          const Node = nodeTypes[(node as typeof node & { type: string }).type ?? 'captured'];
          return (
            <div
              key={node.id}
              data-testid={`node-${node.id}`}
              onClick={() => onNodeClick(null, node)}
              onDoubleClick={() => onNodeDoubleClick(null, node)}
            >
              <Node data={node.data as never} selected={false} />
            </div>
          );
        })}
        {edges.map((edge) => (
          <div
            key={edge.id}
            data-testid={`edge-${edge.id}`}
            className={edge.className}
            data-source={edge.sourceHandle}
            data-motion={edge.data.motion}
          />
        ))}
      </div>
    );
  },
}));
const original = useRunStore.getState();
const scenario = useScenarioStore.getState();
afterEach(() => {
  vi.unstubAllGlobals();
  useRunStore.setState(original, true);
  useScenarioStore.setState(scenario, true);
});
function setup() {
  const events = [
    begin(),
    event('workflow.started', 2, {}, { nodeKey: null }, null),
    event('step.started', 3, { kind: 'http' }),
    response(4),
    event('step.finished', 5),
    event('edge.traversed', 6, {}, { edgeKey: 'sh', nodeKey: 's' }, 'start'),
  ];
  useScenarioStore.setState({ mode: 'view' });
  useRunStore.setState({
    events,
    runId: 'run',
    status: 'running',
    canvasView: 'invocation',
    showRun: true,
    capturedDocument: structuredClone(scenario.document),
    report: null,
  });
  return events;
}
it('renders safe captured positions and node details without allowing graph mutations', () => {
  setup();
  render(
    <>
      <RunCanvas />
      <RunInspector />
    </>
  );
  expect(screen.getByTestId('run-canvas')).toHaveAttribute('data-draggable', 'false');
  expect(screen.getByTestId('run-canvas')).toHaveAttribute('data-connectable', 'false');
  expect(screen.getByTestId('node-h')).toHaveTextContent('completed · HTTP 200');
  expect(screen.getByTestId('edge-sh')).toHaveClass('run-edge-traversed');
  expect(screen.getByTestId('edge-sh')).toHaveAttribute('data-source', 'top');
  expect(screen.queryByText('Scenario')).not.toBeInTheDocument();
  fireEvent.click(screen.getByTestId('node-h'));
  expect(screen.getByLabelText('Run inspector')).toHaveTextContent('Check health');
  expect(screen.getByLabelText('Run inspector')).toHaveTextContent('completed');
});
it('keeps final run results independent of draft edits and navigates parent and repeated child invocations', () => {
  const events = setup();
  const child = {
    workflowKey: 'child',
    invocationId: 'child1',
    parentInvocationId: 'root',
    callerNodeKey: 'a',
    nodeKey: 'ch',
  };
  events.push(
    event('step.started', 7, { kind: 'http' }, child),
    event('step.finished', 8, {}, child),
    event('workflow.finished', 9, { status: 'passed' }, { ...child, nodeKey: null }, null)
  );
  useRunStore.setState({ events: [...events], report: completeReport(events), status: 'passed' });
  render(
    <>
      <RunCanvas />
      <RunSidebar />
    </>
  );
  act(() => useScenarioStore.getState().move('health', { x: 999, y: 999 }));
  expect(runGraph.workflows[0].nodes[1].position).toEqual({ x: 340, y: 160 });
  fireEvent.doubleClick(screen.getByTestId('node-a'));
  expect(screen.getByTestId('node-ch')).toHaveTextContent('Child health');
  fireEvent.click(screen.getByRole('button', { name: 'Back to caller' }));
  expect(screen.getByTestId(`node-${scopedId('iteration', 'h')}`)).toBeInTheDocument();
});
it('keeps accordion expansion and keyboard focus when batched events and the final report arrive', async () => {
  const user = userEvent.setup();
  const events = setup();
  render(<RunPanel />);
  const row = screen.getByRole('button', { name: /Check health.*completed/ });
  row.focus();
  await user.keyboard('{Enter}');
  expect(row).toHaveAttribute('aria-expanded', 'true');
  act(() =>
    useRunStore.setState({
      events: [...events, event('run.finished', 7, { status: 'passed' }, { invocationId: null, nodeKey: null }, null)],
      report: completeReport(events),
      status: 'passed',
    })
  );
  expect(row).toHaveFocus();
  expect(row).toHaveAttribute('aria-expanded', 'true');
  expect(screen.getAllByRole('button', { name: /response.received/ })).toHaveLength(1);
  expect(screen.queryByRole('button', { name: /edge.traversed/ })).not.toBeInTheDocument();
});

it('provides static execution progress under reduced motion', () => {
  setup();
  vi.stubGlobal('matchMedia', () => ({ matches: true, addEventListener: () => {}, removeEventListener: () => {} }));
  render(<RunCanvas />);
  expect(screen.getByTestId('edge-sh')).toHaveAttribute('data-motion', 'false');
  expect(screen.getByTestId('edge-sh')).toHaveClass('run-edge-traversed');
  expect(screen.getByTestId('node-h')).toHaveTextContent('completed');
});
