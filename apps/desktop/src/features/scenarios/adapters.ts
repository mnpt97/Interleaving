import type { Phase } from './lifecycle';
import { MarkerType, type Edge, type Node, type XYPosition } from '@xyflow/react';
import { selectedWorkflow, type ScenarioDocument, type WorkflowNode } from '../../lib/scenario-types';

export type ScenarioFlowNode = Node<{ node: WorkflowNode; phase?:Phase; workflowId?:string }, 'scenario'>;
export function toFlowNodes(
  document: ScenarioDocument,
  selected: string | null,
  workflowId: string | null = null
): ScenarioFlowNode[] {
  const workflow = selectedWorkflow(document, workflowId);
  return (workflow?.nodes ?? []).map((node, index) => ({
    id: node.id,
    type: 'scenario',
    // Layout Y coordinates align node centers, even when node heights differ.
    origin: [0, 0.5],
    deletable: node.type !== 'start' && node.type !== 'end',
    ariaLabel: `${node.type} node ${node.id}`,
    data: { node },
    selected: selected === node.id,
    position: workflow?.layout?.positions[node.id] ?? { x: 80 + index * 260, y: 160 },
  }));
}
export function toFlowEdges(document: ScenarioDocument, workflowId: string | null = null): Edge[] {
  const edges = selectedWorkflow(document, workflowId)?.edges ?? [];
  return edges.map((edge) => {
    const parallel = edges.filter((candidate) => candidate.from === edge.from && candidate.to === edge.to);
    const lane = parallel.findIndex((candidate) => candidate.id === edge.id) - (parallel.length - 1) / 2;
    return {
      id: edge.id,
      source: edge.from,
      target: edge.to,
      sourceHandle: selectedWorkflow(document, workflowId)?.layout?.connections?.[edge.id]?.source ?? 'right',
      targetHandle: selectedWorkflow(document, workflowId)?.layout?.connections?.[edge.id]?.target ?? 'left',
      type: parallel.length > 1 ? 'exclusiveRoute' : 'smoothstep',
      data: { lane: lane * 2 },
      markerEnd: { type: MarkerType.ArrowClosed, width: 18, height: 18 },
      interactionWidth: 24,
      label: edge.default
        ? 'Default'
        : edge.condition?.expression &&
          (edge.condition.expression.length > 72
            ? `${edge.condition.expression.slice(0, 69)}…`
            : edge.condition.expression),
      labelStyle: { fill: 'var(--foreground)', fontSize: 11 },
      labelBgStyle: { fill: 'var(--surface)' },
      ariaLabel: `Connection ${edge.from} to ${edge.to}`,
    };
  });
}
export function roundPosition(position: XYPosition): XYPosition {
  return { x: Number(position.x.toFixed(2)), y: Number(position.y.toFixed(2)) };
}

export function moveNode(
  document: ScenarioDocument,
  id: string,
  position: XYPosition,
  workflowId: string | null = null
): ScenarioDocument {
  const workflow = selectedWorkflow(document, workflowId);
  if (!workflow?.nodes.some((node) => node.id === id) || !Number.isFinite(position.x) || !Number.isFinite(position.y))
    return document;
  position = roundPosition(position);
  const previous = workflow.layout?.positions[id];
  if (previous?.x === position.x && previous.y === position.y) return document;
  return {
    ...document,
    workflows: document.workflows.map((item) =>
      item === workflow
        ? { ...item, layout: { ...item.layout, positions: { ...item.layout?.positions, [id]: position } } }
        : item
    ),
  };
}
