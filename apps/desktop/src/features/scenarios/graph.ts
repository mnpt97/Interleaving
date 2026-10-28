import type { ScenarioDocument, WorkflowDefinition, WorkflowNode } from '../../lib/scenario-types';
import { allocateId } from './ids';

export type InsertKind =
  'http' | 'authentication' | 'extract' | 'setSession' | 'call' | 'exclusiveSplit' | 'exclusiveMerge';
export type InsertTarget = { edgeId?: string; afterNodeId?: string };

export function nodeIdError(workflow: WorkflowDefinition, nodeId: string, nextId: string): string | null {
  if (!nextId.trim() || new TextEncoder().encode(nextId).length > 128) return 'Enter a node ID of 1–128 UTF-8 bytes.';
  if (workflow.nodes.some((node) => node.id === nextId && node.id !== nodeId))
    return 'This node ID is already used in this workflow.';
  return null;
}

/** Rename local references atomically. Literal payloads and other workflows are untouched. */
export function renameNode(workflow: WorkflowDefinition, nodeId: string, nextId: string) {
  const node = workflow.nodes.find((node) => node.id === nodeId);
  if (!node) throw new Error('Node no longer exists.');
  const error = nodeIdError(workflow, nodeId, nextId);
  if (error) throw new Error(error);
  if (nodeId === nextId) return;
  function source(value: unknown) {
    if (!value || typeof value !== 'object' || !('source' in value)) return;
    const item = value as Record<string, unknown>;
    if (['response', 'extraction', 'callOutput'].includes(String(item.source)) && item.nodeId === nodeId)
      item.nodeId = nextId;
    if (item.source === 'object' && item.fields && typeof item.fields === 'object')
      Object.values(item.fields).forEach(source);
    if (item.source === 'array' && Array.isArray(item.items)) item.items.forEach(source);
  }
  for (const current of workflow.nodes) {
    if ((current.type === 'extract' || current.type === 'exclusiveSplit') && current.config.sourceNodeId === nodeId)
      current.config.sourceNodeId = nextId;
    if (current.type === 'call' || current.type === 'exclusiveSplit')
      Object.values(current.config.inputs).forEach(source);
    if (current.type === 'setSession') Object.values(current.config.setHeaders).forEach(source);
    if (current.type === 'http' && current.config.body?.kind === 'jsonTemplate')
      Object.values(current.config.body.fields).forEach(source);
  }
  Object.values(workflow.outputs).forEach((output) => source(output.value));
  node.id = nextId;
  for (const edge of workflow.edges) {
    if (edge.from === nodeId) edge.from = nextId;
    if (edge.to === nodeId) edge.to = nextId;
  }
  if (workflow.layout && Object.prototype.hasOwnProperty.call(workflow.layout.positions, nodeId)) {
    Object.defineProperty(workflow.layout.positions, nextId, {
      value: workflow.layout.positions[nodeId],
      enumerable: true,
      writable: true,
      configurable: true,
    });
    delete workflow.layout.positions[nodeId];
  }
}

/** Immediate interaction checks only. Rust remains authoritative for dataflow and compilation. */
export function connectionError(
  workflow: WorkflowDefinition,
  from: string | null,
  to: string | null,
  replacing?: string
): string | null {
  const source = workflow.nodes.find((node) => node.id === from);
  const target = workflow.nodes.find((node) => node.id === to);
  if (!source || !target) return 'Choose two nodes in the current workflow.';
  if (source.id === target.id) return 'A node cannot connect to itself.';
  if (source.type === 'end' || target.type === 'start')
    return 'Connect an output to an input. Start has no input; End has no output.';
  const edges = workflow.edges.filter((edge) => edge.id !== replacing);
  if (source.type !== 'exclusiveSplit' && edges.some((edge) => edge.from === from && edge.to === to))
    return 'This connection already exists.';
  if (source.type === 'exclusiveSplit' && edges.filter((edge) => edge.from === from).length >= 8)
    return 'An exclusive split supports at most eight routes.';
  // Draft wiring may be incomplete; Validate/Run checks required node degrees.
  const pending = [target.id];
  const visited = new Set<string>();
  while (pending.length) {
    const id = pending.pop()!;
    if (id === source.id) return 'This connection would create a cycle.';
    if (visited.has(id)) continue;
    visited.add(id);
    pending.push(...edges.filter((edge) => edge.from === id).map((edge) => edge.to));
  }
  return null;
}

export function connect(
  workflow: WorkflowDefinition,
  from: string,
  to: string,
  edgeId?: string,
  sourceHandle?: 'left' | 'right' | 'top' | 'bottom',
  targetHandle?: 'left' | 'right' | 'top' | 'bottom'
) {
  if (edgeId && !workflow.edges.some((edge) => edge.id === edgeId)) throw new Error('Connection no longer exists.');
  const previous = workflow.edges.find((edge) => edge.id === edgeId);
  const error = connectionError(workflow, from, to, edgeId);
  if (error) throw new Error(error);
  if (previous) Object.assign(previous, { from, to });
  else
    workflow.edges.push({
      id: allocateId(
        'edge',
        workflow.edges.map((edge) => edge.id)
      ),
      from,
      to,
      ...(workflow.nodes.find((node) => node.id === from)?.type === 'exclusiveSplit'
        ? { condition: { expression: 'false' } }
        : {}),
    });
  const edge = previous ?? workflow.edges[workflow.edges.length - 1];
  if (workflow.nodes.find((node) => node.id === from)?.type !== 'exclusiveSplit') {
    delete edge.condition;
    delete edge.default;
  } else if (!edge.condition && !edge.default) edge.condition = { expression: 'false' };
  const old = workflow.layout?.connections?.[edge.id];
  if (sourceHandle || targetHandle || old)
    workflow.layout = {
      ...workflow.layout,
      positions: workflow.layout?.positions ?? {},
      connections: {
        ...workflow.layout?.connections,
        [edge.id]: { source: sourceHandle ?? old?.source ?? 'right', target: targetHandle ?? old?.target ?? 'left' },
      },
    };
}

function references(value: unknown, ids: Set<string>): boolean {
  if (!value || typeof value !== 'object') return false;
  // Literal JSON is data even when its keys look like source descriptors.
  if ('source' in value && value.source === 'literal') return false;
  if ('kind' in value && (value.kind === 'json' || value.kind === 'text')) return false;
  return Object.entries(value).some(
    ([key, item]) =>
      (['nodeId', 'sourceNodeId'].includes(key) && typeof item === 'string' && ids.has(item)) || references(item, ids)
  );
}

/** Reject external consumers before removing anything; deleting consumers together is safe. */
export function deleteGraphElements(workflow: WorkflowDefinition, nodeIds: string[], edgeIds: string[]) {
  const ids = new Set(nodeIds);
  if (workflow.nodes.some((node) => ids.has(node.id) && (node.type === 'start' || node.type === 'end')))
    throw new Error('Start and End define the workflow boundary and cannot be deleted.');
  const consumers = workflow.nodes.filter((node) => !ids.has(node.id) && references(node, ids));
  const outputs = Object.entries(workflow.outputs).filter(([, output]) => references(output, ids));
  if (consumers.length || outputs.length)
    throw new Error(
      `Node is referenced by ${[...consumers.map((node) => node.id), ...outputs.map(([name]) => `workflow output ${name}`)].join(', ')}. Remove those mappings or delete the dependent nodes together first.`
    );
  workflow.nodes = workflow.nodes.filter((node) => !ids.has(node.id));
  workflow.edges = workflow.edges.filter(
    (edge) => !edgeIds.includes(edge.id) && !ids.has(edge.from) && !ids.has(edge.to)
  );
  for (const id of ids) if (workflow.layout) delete workflow.layout.positions[id];
  if (workflow.layout?.connections)
    for (const id of Object.keys(workflow.layout.connections))
      if (!workflow.edges.some((edge) => edge.id === id)) delete workflow.layout.connections[id];
}

/** Calls are navigation within a VU, never recursive execution or a nested scheduler. */
export function canCall(document: ScenarioDocument, callerId: string, calleeId: string): boolean {
  if (!document.workflows.some((workflow) => workflow.id === calleeId)) return false;
  const pending = [calleeId];
  const visited = new Set<string>();
  while (pending.length) {
    const id = pending.pop()!;
    if (id === callerId) return false;
    if (visited.has(id)) continue;
    visited.add(id);
    for (const node of document.workflows.find((workflow) => workflow.id === id)?.nodes ?? [])
      if (node.type === 'call') pending.push(node.config.workflowId);
  }
  return true;
}

/** Insert on an explicit edge or append to an unconnected output, in one history entry. */
export function insertNode(
  document: ScenarioDocument,
  workflow: WorkflowDefinition,
  kind: InsertKind,
  target: InsertTarget,
  calleeId?: string
) {
  const candidates = target.edgeId
    ? workflow.edges.filter((edge) => edge.id === target.edgeId)
    : target.afterNodeId
      ? workflow.edges.filter((edge) => edge.from === target.afterNodeId)
      : workflow.edges.filter((edge) => workflow.nodes.some((node) => node.id === edge.to && node.type === 'end'));
  if (candidates.length > 1) throw new Error('Choose an explicit insertion connection; this output is ambiguous.');
  const edge = candidates[0];
  if (target.edgeId && !edge) throw new Error('Insertion connection no longer exists.');
  if (!edge && !target.afterNodeId) throw new Error('Choose a connection or an unconnected output to add a node.');
  const from = edge?.from ?? target.afterNodeId!;
  const source = workflow.nodes.find((node) => node.id === from);
  if (!source || source.type === 'end') throw new Error('Choose an output on a supported node.');
  if (kind === 'call' && (!calleeId || !canCall(document, workflow.id, calleeId)))
    throw new Error('Choose a workflow that does not create a recursive call.');
  const id = allocateId(
    kind === 'authentication' ? 'authenticate' : kind,
    workflow.nodes.map((node) => node.id)
  );
  const previousHttp = () => {
    let current: WorkflowNode | undefined = source;
    const visited = new Set<string>();
    while (current && !visited.has(current.id)) {
      if (current.type === 'http') return current.id;
      visited.add(current.id);
      const incoming = workflow.edges.find((edge) => edge.to === current!.id);
      current = workflow.nodes.find((node) => node.id === incoming?.from);
    }
    return '';
  };
  if (kind === 'exclusiveSplit') {
    if (!edge) throw new Error('Choose an existing connection for the split/merge pair.');
    const mergeId = allocateId('merge', [...workflow.nodes.map((node) => node.id), id]);
    workflow.nodes.push(
      { id, type: 'exclusiveSplit', config: { sourceNodeId: previousHttp(), inputs: {} } },
      { id: mergeId, type: 'exclusiveMerge', config: {} }
    );
    const destination = edge.to;
    edge.to = id;
    const add = (from: string, to: string, route: Partial<WorkflowDefinition['edges'][number]> = {}) => {
      workflow.edges.push({
        id: allocateId(
          'edge',
          workflow.edges.map((edge) => edge.id)
        ),
        from,
        to,
        ...route,
      });
    };
    add(id, mergeId, { condition: { expression: 'response.status >= 200 and response.status < 300' } });
    add(id, mergeId, { default: true });
    add(mergeId, destination);
    const positions = { ...workflow.layout?.positions };
    const origin = positions[from] ?? { x: 80, y: 160 };
    const visited = new Set<string>();
    const pending = [destination];
    while (pending.length) {
      const key = pending.pop()!;
      if (visited.has(key)) continue;
      visited.add(key);
      const old = positions[key] ?? { x: origin.x + 260, y: origin.y };
      positions[key] = { ...old, x: old.x + 520 };
      pending.push(...workflow.edges.filter((edge) => edge.from === key).map((edge) => edge.to));
    }
    positions[id] = { x: origin.x + 260, y: origin.y };
    positions[mergeId] = { x: origin.x + 520, y: origin.y };
    workflow.layout = { ...workflow.layout, positions };
    return;
  }
  const node = createNode(kind, id, previousHttp(), calleeId);
  const index = edge ? workflow.nodes.findIndex((node) => node.id === edge.to) : workflow.nodes.length;
  workflow.nodes.splice(index < 0 ? workflow.nodes.length : index, 0, node);
  const destination = edge?.to;
  if (edge?.condition || edge?.default !== undefined) edge.to = id;
  else {
    if (edge) workflow.edges = workflow.edges.filter((item) => item.id !== edge.id);
    connect(workflow, from, id);
  }
  if (destination) connect(workflow, id, destination);
  const positions = { ...workflow.layout?.positions };
  const origin = positions[from] ?? { x: 80 + workflow.nodes.findIndex((node) => node.id === from) * 260, y: 160 };
  const pending = destination ? [destination] : [];
  const visited = new Set<string>();
  while (pending.length) {
    const key = pending.pop()!;
    if (visited.has(key) || key === id) continue;
    visited.add(key);
    const position = positions[key] ?? { x: origin.x + 260, y: origin.y };
    positions[key] = { ...position, x: position.x + 260 };
    pending.push(...workflow.edges.filter((edge) => edge.from === key).map((edge) => edge.to));
  }
  positions[id] = { x: origin.x + 260, y: origin.y };
  workflow.layout = { ...workflow.layout, positions };
}

/** Shared defaults for independently placed nodes and explicit insertion commands. */
function createNode(kind: InsertKind, id: string, sourceNodeId: string, calleeId?: string): WorkflowNode {
  let node: WorkflowNode;
  switch (kind) {
    case 'exclusiveSplit':
      node = { id, type: 'exclusiveSplit', config: { sourceNodeId, inputs: {} } };
      break;
    case 'exclusiveMerge':
      node = { id, type: 'exclusiveMerge', config: {} };
      break;
    case 'http':
      node = { id, type: 'http', name: 'HTTP Request', config: { method: 'GET', path: '', headers: {} } };
      break;
    case 'authentication':
      node = {
        id,
        type: 'authentication',
        name: 'Authentication',
        config: {
          method: 'POST',
          path: '/auth/login',
          mode: 'bearer',
          username: { source: 'env', name: 'INTERL_USERNAME' },
          password: { source: 'env', name: 'INTERL_PASSWORD' },
          usernameKey: 'username',
          passwordKey: 'password',
          tokenExpression: 'response.body.accessToken',
          headerName: 'Authorization',
          headerPrefix: 'Bearer ',
        },
      };
      break;
    case 'extract':
      node = {
        id,
        type: 'extract',
        config: { sourceNodeId, outputs: { token: { expression: 'response.body.token', type: 'string' } } },
      };
      break;
    case 'setSession':
      node = { id, type: 'setSession', config: { setHeaders: {}, removeHeaders: [] } };
      break;
    case 'call':
      node = { id, type: 'call', config: { workflowId: calleeId!, inputs: {} } };
      break;
  }
  return node;
}

/** Add one node without changing any existing node, edge, or position. */
export function addNode(
  document: ScenarioDocument,
  workflow: WorkflowDefinition,
  kind: InsertKind,
  nearNodeId?: string,
  calleeId?: string
) {
  if (kind === 'call' && (!calleeId || !canCall(document, workflow.id, calleeId)))
    throw new Error('Choose a nonrecursive workflow.');
  const id = allocateId(
    kind === 'authentication' ? 'authenticate' : kind,
    workflow.nodes.map((node) => node.id)
  );
  const source = workflow.nodes.find((node) => node.id === nearNodeId);
  const node = createNode(kind, id, source?.type === 'http' ? source.id : '', calleeId);
  const positions = { ...workflow.layout?.positions };
  const origin = nearNodeId ? positions[nearNodeId] : undefined;
  let position = origin
    ? { x: origin.x + 260, y: origin.y }
    : { x: Math.max(80, ...Object.values(positions).map((point) => point.x)) + 260, y: 160 };
  while (
    Object.values(positions).some(
      (point) => Math.abs(point.x - position.x) < 200 && Math.abs(point.y - position.y) < 120
    )
  )
    position = { ...position, y: position.y + 140 };
  workflow.nodes.push(node);
  positions[id] = position;
  workflow.layout = { ...workflow.layout, positions };
}

/** Type replacement keeps graph identity; producer references must be removed first. */
export function changeNode(
  document: ScenarioDocument,
  workflow: WorkflowDefinition,
  nodeId: string,
  kind: InsertKind,
  calleeId?: string
) {
  const index = workflow.nodes.findIndex((node) => node.id === nodeId);
  const previous = workflow.nodes[index];
  if (!previous || ['start', 'end'].includes(previous.type)) throw new Error('Start and End cannot change type.');
  if (previous.type === kind) return;
  const ids = new Set([nodeId]);
  const consumers = workflow.nodes
    .filter((node) => node.id !== nodeId)
    .map((node) => {
      return node;
    });
  if (
    consumers.some((node) => references(node, ids)) ||
    Object.values(workflow.outputs).some((out) => references(out, ids))
  )
    throw new Error('Remove output references first.');
  if (kind === 'call' && (!calleeId || !canCall(document, workflow.id, calleeId)))
    throw new Error('Choose a nonrecursive workflow.');
  let sourceId = '';
  let current = nodeId;
  const seen = new Set<string>();
  while (!seen.has(current)) {
    seen.add(current);
    const incoming = workflow.edges.filter((edge) => edge.to === current);
    if (incoming.length !== 1) break;
    current = incoming[0].from;
    if (workflow.nodes.some((node) => node.id === current && node.type === 'http')) {
      sourceId = current;
      break;
    }
  }
  const replacement = createNode(kind, nodeId, sourceId, calleeId);
  if (previous.name) replacement.name = previous.name;
  if (
    (replacement.type === 'http' || replacement.type === 'authentication') &&
    (previous.type === 'http' || previous.type === 'authentication')
  ) {
    if (previous.config.baseUrl) replacement.config.baseUrl = previous.config.baseUrl;
  }
  if (previous.type === 'exclusiveSplit' && kind !== 'exclusiveSplit')
    for (const edge of workflow.edges.filter((edge) => edge.from === nodeId)) {
      delete edge.condition;
      delete edge.default;
    }
  if (kind === 'exclusiveSplit')
    for (const edge of workflow.edges.filter((edge) => edge.from === nodeId)) edge.condition = { expression: 'false' };
  workflow.nodes[index] = replacement;
}
