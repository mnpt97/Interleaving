import { emptyEventContext } from '../../lib/run-types';
import { useState, type ComponentType } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import source from '../../../../../scenarios/examples/auth-login.interl.json?raw';
import { documentSchema } from '../../lib/scenario-types';
import { ScenarioCanvas } from './ScenarioCanvas';
import { useScenarioStore } from './store';
import { useRunStore } from '../runs/store';
import type { ScenarioFlowNode } from './adapters';

vi.mock('../../lib/theme', () => ({ useTheme: () => ({ resolvedTheme: 'dark' }) }));
vi.mock('@xyflow/react', () => ({
  Position: { Left: 'left', Right: 'right' },
  Handle: ({ type }: { type: string }) => <span data-testid={`${type}-handle`} />,
  Background: () => null,
  Controls: () => null,
  applyNodeChanges: (_changes: unknown, nodes: unknown) => nodes,
  useNodesState: (initial: ScenarioFlowNode[]) => useState(initial),
  // Exercise our real custom node and callbacks without simulating React Flow's DOM measurements.
  ReactFlow: ({
    nodes,
    nodeTypes,
    onNodeClick,
  }: {
    nodes: ScenarioFlowNode[];
    nodeTypes: { scenario: ComponentType<{ data: ScenarioFlowNode['data']; selected: boolean }> };
    onNodeClick: (event: unknown, node: ScenarioFlowNode) => void;
  }) => {
    const Node = nodeTypes.scenario;
    return (
      <div>
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
  useScenarioStore.setState(scenarioInitial, true);
  useRunStore.setState(runInitial, true);
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
  expect(extract.querySelectorAll('[data-testid$="-handle"]')).toHaveLength(2);
  const session = screen.getByTestId('apply-token');
  expect(session).toHaveTextContent('Set session');
  expect(session).toHaveTextContent('1 set · 0 removed');
  expect(session.querySelectorAll('[data-testid$="-handle"]')).toHaveLength(2);
  fireEvent.click(session);
  expect(useScenarioStore.getState()).toMatchObject({ selectedId: 'apply-token', inspectorOpen: true });
});

it('shows auth step failure state only for the matching scenario', () => {
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
  expect(screen.getByTestId('extract-token')).toHaveTextContent('Stopped');
  unmount();
  useRunStore.setState({ scenarioId: 'other-scenario' });
  render(<ScenarioCanvas />);
  expect(screen.getByTestId('extract-token')).not.toHaveTextContent('Stopped');
});
