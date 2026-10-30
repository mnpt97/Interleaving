import { useState, type ComponentType, type ReactNode } from 'react';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { Node, ReactFlowProps } from '@xyflow/react';
import { LifecycleCanvas } from './LifecycleCanvas';
import { useScenarioStore } from './store';
import { executeCommand, newDraft } from './draft';
import { phaseWorkflow, scopedId } from './lifecycle';

const captured = vi.hoisted(() => ({ props: {} as ReactFlowProps<Node> }));
vi.mock('../../lib/theme', () => ({ useTheme: () => ({ resolvedTheme: 'dark' }) }));
vi.mock('./ScenarioNode', () => ({ ScenarioNode: () => null }));
vi.mock('./AddNodePopover', () => ({ AddNodePopover: () => null }));
vi.mock('./InitialCanvasFit', () => ({ InitialCanvasFit: () => null }));
vi.mock('@xyflow/react', () => ({
  Position: { Left: 'left', Right: 'right', Top: 'top', Bottom: 'bottom' },
  MarkerType: { ArrowClosed: 'arrow' },
  Handle: () => null,
  Background: () => null,
  Controls: () => null,
  ControlButton: () => null,
  useNodesState: (nodes: Node[]) => useState(nodes),
  applyNodeChanges: (_: unknown, nodes: Node[]) => nodes,
  NodeResizeControl: ({
    onResize,
    children,
  }: {
    onResize: (event: unknown, size: { width: number; height: number }) => void;
    children: ReactNode;
  }) => <button onClick={() => onResize(null, { width: 1000, height: 600 })}>{children}</button>,
  ReactFlow: (props: ReactFlowProps<Node>) => {
    captured.props = props;
    return (
      <>
        {props.nodes
          ?.filter((node) => node.type === 'phase')
          .map((node) => {
            const Frame = props.nodeTypes!.phase as ComponentType<{ data: Record<string, unknown> }>;
            return <Frame key={node.id} data={node.data} />;
          })}
      </>
    );
  },
}));
const original = useScenarioStore.getState();
afterEach(() => {
  cleanup();
  useScenarioStore.setState(original, true);
  localStorage.clear();
});

function setup() {
  const document = executeCommand(newDraft(), { type: 'phase.node.add', phase: 'iteration', kind: 'http' });
  const workflow = phaseWorkflow(document, 'iteration')!;
  useScenarioStore.setState({
    document,
    mode: 'edit',
    pendingChange: false,
    busy: false,
    selectedPhase: 'iteration',
    selectedId: 'http-1',
    selectedWorkflowId: workflow.id,
    formErrors: { [`http:${workflow.id}:http-1:path`]: 'Enter a request path.' },
  });
  render(<LifecycleCanvas />);
  return workflow.id;
}

it('allows moving a new node before entering its path, without automatically growing its box', () => {
  const workflowId = setup();
  expect(captured.props.nodesDraggable).toBe(true);
  const size = captured.props.nodes!.find((node) => node.id === 'phase:iteration')!.style;
  const node = captured.props.nodes!.find((node) => node.id === scopedId('iteration', 'http-1'))!;
  act(() => captured.props.onNodeDragStop?.({} as never, { ...node, position: { x: 90, y: 180 } }, [node]));
  const workflow = useScenarioStore.getState().document!.workflows.find((workflow) => workflow.id === workflowId)!;
  expect(workflow.layout!.positions['http-1']).toEqual({ x: 610, y: 200 });
  expect(captured.props.nodes!.find((node) => node.id === 'phase:iteration')!.style).toEqual(size);
});

it('moves a blank HTTP node into Initialize by dragging across the phase boundary', () => {
  setup();
  const source = captured.props.nodes!.find((node) => node.id === 'phase:iteration')!;
  const target = captured.props.nodes!.find((node) => node.id === 'phase:initialize')!;
  const node = captured.props.nodes!.find((node) => node.id === scopedId('iteration', 'http-1'))!;
  act(() =>
    captured.props.onNodeDragStop?.(
      {} as never,
      { ...node, position: { x: 240, y: target.position.y + 120 - source.position.y } },
      [node]
    )
  );
  expect(useScenarioStore.getState().error).toBeNull();
  expect(useScenarioStore.getState().selectedPhase).toBe('initialize');
  const workflow = phaseWorkflow(useScenarioStore.getState().document!, 'initialize')!;
  expect(workflow.nodes.some((node) => node.id === 'http-1')).toBe(true);
  expect(workflow.layout!.positions['http-1']).toEqual({ x: 0, y: 160 });
  expect(workflow.edges).toEqual([]);
});

it('resizes the phase independently of incomplete fields', () => {
  setup();
  fireEvent.click(screen.getByRole('button', { name: 'Resize Iteration' }));
  expect(captured.props.nodes!.find((node) => node.id === 'phase:iteration')!.style).toEqual({
    width: 1000,
    height: 600,
    pointerEvents: 'none',
  });
});

it('moves phase boxes by their header while keeping workflow coordinates unchanged', () => {
  setup();
  const before = useScenarioStore.getState().document;
  const frame = captured.props.nodes!.find((node) => node.id === 'phase:iteration')!;
  expect(frame.draggable).toBe(true);
  expect(frame.dragHandle).toBe('.phase-heading');
  act(() => captured.props.onNodeDragStop?.({} as never, { ...frame, position: { x: 415, y: 795 } }, [frame]));
  expect(captured.props.nodes!.find((node) => node.id === frame.id)!.position).toEqual({ x: 420, y: 800 });
  expect(useScenarioStore.getState().document).toBe(before);
});

it('keeps phase boxes and their nodes immovable in View mode', () => {
  setup();
  act(() => useScenarioStore.setState({ mode: 'view' }));
  expect(captured.props.nodesDraggable).toBe(false);
  expect(captured.props.nodes!.every((node) => !node.draggable)).toBe(true);
});

it('aligns node centers to the entry/exit circle row despite off-grid measured bounds', () => {
  setup();
  const node = captured.props.nodes!.find((node) => node.id === scopedId('iteration', 'http-1'))!;
  const entry = captured.props.nodes!.find((node) => node.id === scopedId('iteration', 'start'))!;
  act(() =>
    captured.props.onNodeDragStop?.({} as never, { ...node, position: { x: 240, y: entry.position.y + 3.5 } }, [node])
  );
  const aligned = captured.props.nodes!.find((value) => value.id === node.id)!;
  expect(aligned.position.y).toBe(entry.position.y);
  expect(aligned.origin?.[1]).toBe(entry.origin?.[1]);
});

it('rejects cross-phase moves with unsaved configuration without losing its fields or graph', () => {
  setup();
  act(() => useScenarioStore.getState().setFormError('configuration:http:pending', 'Unsaved headers'));
  const before = useScenarioStore.getState().document;
  const source = captured.props.nodes!.find((node) => node.id === 'phase:iteration')!;
  const target = captured.props.nodes!.find((node) => node.id === 'phase:cleanup')!;
  const node = captured.props.nodes!.find((node) => node.id === scopedId('iteration', 'http-1'))!;
  act(() =>
    captured.props.onNodeDragStop?.(
      {} as never,
      { ...node, position: { x: 240, y: target.position.y + 120 - source.position.y } },
      [node]
    )
  );
  expect(useScenarioStore.getState().document).toBe(before);
  expect(useScenarioStore.getState().formErrors['configuration:http:pending']).toBe('Unsaved headers');
  expect(captured.props.nodes!.find((value) => value.id === node.id)!.position).toEqual(node.position);
});
