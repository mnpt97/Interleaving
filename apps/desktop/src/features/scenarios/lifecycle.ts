import type { ScenarioDocument, WorkflowDefinition } from '../../lib/scenario-types';
import { allocateId } from './ids';
import { addNode, canCall, deleteGraphElements, type InsertKind } from './graph';

export const phases = ['initialize', 'iteration', 'cleanup'] as const;
export type Phase = (typeof phases)[number];
export const phaseName = { initialize: 'Initialize', iteration: 'Iteration', cleanup: 'Cleanup' };
export function phaseWorkflow(document: ScenarioDocument, phase: Phase) {
  const binding = document.groups[0]?.lifecycle[phase];
  return document.workflows.find((workflow) => workflow.id === binding?.workflowId);
}
/** Empty required fields already represented in the node do not contain unsaved inspector text. */
export function canMovePhaseFields(
  document: ScenarioDocument,
  errors: Record<string, string>,
  phase: Phase,
  ids: string[]
) {
  const workflow = phaseWorkflow(document, phase);
  return Object.keys(errors).every((key) =>
    workflow?.nodes.some(
      (node) =>
        ids.includes(node.id) &&
        node.type === 'http' &&
        !node.config.path.trim() &&
        key === `http:${workflow.id}:${node.id}:path`
    )
  );
}
/** Only exclusively lifecycle-owned definitions are edited inline. */
export function ownsPhase(document: ScenarioDocument, phase: Phase): boolean {
  const workflow = phaseWorkflow(document, phase);
  if (!workflow) return true;
  const uses = document.groups
    .flatMap((group) => phases.map((phase) => group.lifecycle[phase]?.workflowId))
    .filter((id) => id === workflow.id).length;
  return (
    uses === 1 &&
    !document.workflows.some((flow) =>
      flow.nodes.some((node) => node.type === 'call' && node.config.workflowId === workflow.id)
    )
  );
}
function emptyWorkflow(id: string, name: string): WorkflowDefinition {
  return {
    id,
    name,
    inputs: {},
    outputs: {},
    nodes: [
      { id: 'start', type: 'start' },
      { id: 'end', type: 'end' },
    ],
    edges: [],
    layout: { positions: { start: { x: 80, y: 160 }, end: { x: 600, y: 160 } } },
  };
}
export function ensurePhase(document: ScenarioDocument, phase: Phase): WorkflowDefinition {
  const existing = phaseWorkflow(document, phase);
  if (existing) {
    if (!ownsPhase(document, phase))
      throw new Error('Open the shared workflow or detach a copy before editing this phase.');
    return existing;
  }
  const group = document.groups[0];
  if (!group) throw new Error('Configure a workload first.');
  const workflow = emptyWorkflow(
    allocateId(
      'workflow',
      document.workflows.map((flow) => flow.id)
    ),
    phaseName[phase]
  );
  document.workflows.push(workflow);
  group.lifecycle[phase] = { workflowId: workflow.id, inputs: {} };
  return workflow;
}
export function detachPhase(document: ScenarioDocument, phase: Phase) {
  const source = phaseWorkflow(document, phase);
  if (!source) {
    ensurePhase(document, phase);
    return;
  }
  if (ownsPhase(document, phase)) return;
  const copy = structuredClone(source);
  copy.id = allocateId(
    'workflow',
    document.workflows.map((flow) => flow.id)
  );
  copy.name = `${phaseName[phase]} · ${source.name}`;
  document.workflows.push(copy);
  const group = document.groups[0];
  group.lifecycle[phase] = { ...group.lifecycle[phase]!, workflowId: copy.id };
}
/** Walk descriptors only; literal payloads never create executable dependencies. */
function sources(value: unknown, visit: (value: Record<string, unknown>) => void) {
  if (!value || typeof value !== 'object') return;
  const item = value as Record<string, unknown>;
  if (item.source === 'literal' || item.kind === 'json' || item.kind === 'text') return;
  if (typeof item.source === 'string') visit(item);
  Object.values(item).forEach((child) => sources(child, visit));
}
function checkGatewaySelection(workflow: WorkflowDefinition, ids: Set<string>) {
  const next = (id: string) => workflow.edges.filter((edge) => edge.from === id).map((edge) => edge.to);
  const reaches = (from: string, to: string) => {
    const queue = [from],
      seen = new Set<string>();
    while (queue.length) {
      const id = queue.pop()!;
      if (id === to) return true;
      if (seen.has(id)) continue;
      seen.add(id);
      queue.push(...next(id));
    }
    return false;
  };
  for (const split of workflow.nodes.filter((node) => node.type === 'exclusiveSplit')) {
    const allPaths = (from: string, to: string, path = new Set<string>()): boolean => {
      if (from === to) return true;
      if (path.has(from)) return false;
      const successors = next(from);
      if (!successors.length) return false;
      const visited = new Set(path).add(from);
      return successors.every((id) => allPaths(id, to, visited));
    };
    const candidates = workflow.nodes.filter(
      (node) =>
        node.type === 'exclusiveMerge' &&
        next(split.id).length > 1 &&
        next(split.id).every((id) => allPaths(id, node.id))
    );
    const merge = candidates.find(
      (candidate) => !candidates.some((other) => other.id !== candidate.id && reaches(other.id, candidate.id))
    );
    const region = new Set<string>();
    const queue = [split.id];
    while (queue.length) {
      const id = queue.pop()!;
      if (region.has(id)) continue;
      region.add(id);
      if (id !== merge?.id) queue.push(...next(id));
    }
    if ([...region].some((id) => ids.has(id)) && (!merge || [...region].some((id) => !ids.has(id))))
      throw new Error('Move the whole gateway region, including its split, branches, and merge.');
  }
  if (
    workflow.nodes.some((node) => ids.has(node.id) && node.type === 'exclusiveMerge') &&
    !workflow.nodes.some((node) => ids.has(node.id) && node.type === 'exclusiveSplit')
  )
    throw new Error('Move the matching split and branch nodes with this merge.');
}

/** Move a closed dependency set in one history command; executable gaps stay explicit. */
export function moveToPhase(document: ScenarioDocument, from: Phase, to: Phase, nodeIds: string[]) {
  if (from === to) throw new Error('Choose another phase.');
  const source = phaseWorkflow(document, from);
  if (!source || !ownsPhase(document, from)) throw new Error('Detach the shared source phase first.');
  const ids = new Set(nodeIds);
  const moving = source.nodes.filter((node) => ids.has(node.id));
  if (
    !moving.length ||
    moving.length !== ids.size ||
    moving.some((node) => node.type === 'start' || node.type === 'end')
  )
    throw new Error('Select existing non-boundary nodes to move.');
  const target = ensurePhase(document, to);
  if (moving.some((node) => target.nodes.some((other) => other.id === node.id)))
    throw new Error('A node ID is already used in the destination. Rename it before moving.');
  checkGatewaySelection(source, ids);
  for (const node of moving) {
    if ((node.type === 'extract' || node.type === 'exclusiveSplit') && !ids.has(node.config.sourceNodeId))
      throw new Error('Move the source HTTP node with its dependent nodes.');
    sources(node, (reference) => {
      if (reference.source === 'input') throw new Error('Remove workflow-input mappings before moving these nodes.');
      if (
        ['extraction', 'response', 'callOutput'].includes(String(reference.source)) &&
        !ids.has(String(reference.nodeId))
      )
        throw new Error('Move dependent producers and consumers together.');
    });
    if (node.type === 'call' && !canCall(document, target.id, node.config.workflowId))
      throw new Error('This move would create a recursive call.');
  }
  const edges = source.edges.filter((edge) => ids.has(edge.from) && ids.has(edge.to));
  const positions = Object.fromEntries(
    moving.map((node, index) => [node.id, source.layout?.positions[node.id] ?? { x: 340 + index * 260, y: 160 }])
  );
  const attachments = Object.fromEntries(
    edges.flatMap((edge) =>
      source.layout?.connections?.[edge.id] ? [[edge.id, source.layout.connections[edge.id]]] : []
    )
  );
  deleteGraphElements(source, nodeIds, []); // Protect remaining consumers and published outputs.
  if (
    from !== 'iteration' &&
    source.nodes.every((node) => node.type === 'start' || node.type === 'end') &&
    !Object.keys(source.outputs).length
  ) {
    document.groups[0].lifecycle[from] = null;
    document.workflows = document.workflows.filter((workflow) => workflow.id !== source.id);
  }

  const offset =
    Math.max(80, ...Object.values(target.layout?.positions ?? {}).map((position) => position.x)) +
    260 -
    Math.min(...Object.values(positions).map((position) => position.x));
  target.nodes.splice(
    Math.max(
      0,
      target.nodes.findIndex((node) => node.type === 'end')
    ),
    0,
    ...moving
  );
  target.layout ??= { positions: {} };
  for (const [id, position] of Object.entries(positions))
    target.layout.positions[id] = { ...position, x: position.x + offset };
  for (const edge of edges) {
    const id = target.edges.some((other) => other.id === edge.id)
      ? allocateId(
          'edge',
          target.edges.map((edge) => edge.id)
        )
      : edge.id;
    target.edges.push({ ...edge, id });
    if (attachments[edge.id]) target.layout.connections = { ...target.layout.connections, [id]: attachments[edge.id] };
  }
}
export function addPhaseNode(document: ScenarioDocument, phase: Phase, kind: InsertKind, calleeId?: string) {
  const workflow = ensurePhase(document, phase);
  // Empty phase pass-through is presentation only once its first operation is authored.
  if (workflow.nodes.every((node) => node.type === 'start' || node.type === 'end')) workflow.edges = [];
  addNode(document, workflow, kind, undefined, calleeId);
  return workflow;
}
// React Flow interpolates IDs into quoted CSS selectors when updating node handles.
// URI encoding preserves the tuple without exposing quotes or backslashes to that selector.
export const scopedId = (phase: string, id: string) => encodeURIComponent(JSON.stringify([phase, id]));
export function localId(id: string): { phase: Phase; id: string } | null {
  try {
    const [phase, value] = JSON.parse(decodeURIComponent(id));
    return phases.includes(phase) && typeof value === 'string' ? { phase, id: value } : null;
  } catch {
    return null;
  }
}
export type PhaseBox = {
  phase: Phase;
  x: number;
  y: number;
  width: number;
  height: number;
  minX: number;
  minY: number;
  workflow?: WorkflowDefinition;
  inline: boolean;
};
/** Fixed phase order plus derived bounds needs no new persisted layout schema. */
export function phaseContentBounds(positions: { x: number; y: number }[], reference = false) {
  if (!positions.length) return { minX: 0, minY: 80, width: reference ? 360 : 320, height: reference ? 220 : 180 };
  const minX = Math.min(...positions.map((position) => position.x));
  const minY = Math.min(...positions.map((position) => position.y)) - 100;
  return {
    minX,
    minY,
    width: Math.ceil((Math.max(...positions.map((position) => position.x)) - minX + 350) / 10) * 10,
    height: Math.ceil((Math.max(...positions.map((position) => position.y)) - minY + 140) / 10) * 10,
  };
}
export function phaseBoxes(document: ScenarioDocument): PhaseBox[] {
  let y = 140;
  const boxes = phases.map((phase) => {
    const workflow = phaseWorkflow(document, phase);
    const inline = ownsPhase(document, phase);
    const content = inline
      ? (workflow?.nodes.filter((node) => node.type !== 'start' && node.type !== 'end') ?? [])
      : [];
    const positions = content.map(
      (node, index) => workflow?.layout?.positions[node.id] ?? { x: 340 + index * 260, y: 160 }
    );
    const { minX, minY, width, height } = phaseContentBounds(positions, !inline);
    const box = { phase, workflow, inline, x: 80, y, width, height, minX, minY };
    y += height + 100;
    return box;
  });
  const widest = Math.max(...boxes.map((box) => box.width));
  return boxes.map((box) => ({ ...box, x: 80 + (widest - box.width) / 2 }));
}
