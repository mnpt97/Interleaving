import type { Edge, Node, XYPosition } from "@xyflow/react";
import type { ScenarioDocument, WorkflowNode } from "../../lib/scenario-types";

export type ScenarioFlowNode = Node<{ node: WorkflowNode }, "scenario">;
export function toFlowNodes(document: ScenarioDocument, selected: string | null): ScenarioFlowNode[] {
  return document.workflow.nodes.map((node, index) => ({ id: node.id, type: "scenario", data: { node }, selected: selected === node.id, position: document.layout?.positions[node.id] ?? { x: 80 + index * 260, y: 160 } }));
}
export function toFlowEdges(document: ScenarioDocument): Edge[] {
  return document.workflow.edges.map((edge, index) => ({ id: `edge-${index}`, source: edge.from, target: edge.to, type: "smoothstep" }));
}
export function moveNode(document: ScenarioDocument, id: string, position: XYPosition): ScenarioDocument {
  if (!document.workflow.nodes.some((node) => node.id === id) || !Number.isFinite(position.x) || !Number.isFinite(position.y)) return document;
  return { ...document, layout: { positions: { ...document.layout?.positions, [id]: position } } };
}
