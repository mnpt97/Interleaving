import type { Edge, Node, XYPosition } from '@xyflow/react';
import { iterationWorkflow, type ScenarioDocument, type WorkflowNode } from '../../lib/scenario-types';

export type ScenarioFlowNode = Node<{ node: WorkflowNode }, 'scenario'>;
export function toFlowNodes(document: ScenarioDocument, selected: string | null): ScenarioFlowNode[] {
  const workflow = iterationWorkflow(document);
  return (workflow?.nodes ?? []).map((node, index) => ({
    id: node.id,
    type: 'scenario',
    data: { node },
    selected: selected === node.id,
    position: workflow?.layout?.positions[node.id] ?? { x: 80 + index * 260, y: 160 },
  }));
}
export function toFlowEdges(document: ScenarioDocument): Edge[] {
  return (iterationWorkflow(document)?.edges ?? []).map((edge) => ({
    id: edge.id,
    source: edge.from,
    target: edge.to,
    type: 'smoothstep',
  }));
}
export function moveNode(document: ScenarioDocument, id: string, position: XYPosition): ScenarioDocument {
  const workflow = iterationWorkflow(document);
  if (!workflow?.nodes.some((node) => node.id === id) || !Number.isFinite(position.x) || !Number.isFinite(position.y))
    return document;
  return {
    ...document,
    workflows: document.workflows.map((item) =>
      item === workflow ? { ...item, layout: { positions: { ...item.layout?.positions, [id]: position } } } : item
    ),
  };
}
