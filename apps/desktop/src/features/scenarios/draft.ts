import { addPhaseNode, detachPhase, ensurePhase, moveToPhase, type Phase } from './lifecycle';
import {
  documentSchema,
  type ScenarioDocument,
  type WorkflowDefinition,
  type WorkflowNode,
} from '../../lib/scenario-types';
import {
  configureScenario,
  configureWorkflow,
  checkExtractionOutputs,
  type ScenarioConfiguration,
  type WorkflowConfiguration,
} from './configuration';
import { allocateId } from './ids';
import {
  addNode,
  changeNode,
  renameNode,
  connect,
  deleteGraphElements,
  insertNode,
  type InsertKind,
  type InsertTarget,
} from './graph';
export { allocateId } from './ids';
import { insertAuthentication, configureAuthentication, type AuthenticationSettings } from './authentication';
export type EditorCommand =
  | { type: 'phase.create'; phase: Phase }
  | { type: 'phase.detach'; phase: Phase }
  | { type: 'phase.node.add'; phase: Phase; kind: InsertKind; calleeId?: string; newWorkflowName?: string }
  | { type: 'phase.move'; from: Phase; to: Phase; nodeIds: string[]; position?: { x: number; y: number } }
  | { type: 'node.move'; workflowId: string; nodeId: string; position: { x: number; y: number } }
  | { type: 'node.rename'; workflowId: string; nodeId: string; nextId: string; name?: string }
  | { type: 'scenario.configure'; configuration: ScenarioConfiguration }
  | { type: 'workflow.configure'; workflowId: string; configuration: WorkflowConfiguration }
  | { type: 'name'; name: string }
  | { type: 'baseUrl'; baseUrl: string }
  | { type: 'authentication.insert'; workflowId: string; edgeId?: string }
  | { type: 'authentication.configure'; workflowId: string; nodeId: string; settings: AuthenticationSettings }
  | { type: 'http.insert'; workflowId: string; edgeId?: string }
  | { type: 'workflow.create'; name: string }
  | { type: 'workflow.duplicate'; workflowId: string }
  | { type: 'workflow.delete'; workflowId: string }
  | { type: 'workflow.update'; workflowId: string; value: WorkflowDefinition }
  | ({
      type: 'node.insert' | 'node.add';
      workflowId: string;
      kind: InsertKind;
      calleeId?: string;
      newWorkflowName?: string;
    } & InsertTarget)
  | {
      type: 'gateway.configure';
      workflowId: string;
      nodeId: string;
      sourceNodeId: string;
      inputs?: Extract<WorkflowNode, { type: 'exclusiveSplit' }>['config']['inputs'];
      routes: WorkflowDefinition['edges'];
    }
  | { type: 'boundary.add'; workflowId: string; kind: 'start' | 'end' }
  | {
      type: 'edge.connect';
      workflowId: string;
      from: string;
      to: string;
      sourceHandle?: 'left' | 'right' | 'top' | 'bottom';
      targetHandle?: 'left' | 'right' | 'top' | 'bottom';
    }
  | {
      type: 'edge.reconnect';
      workflowId: string;
      edgeId: string;
      from: string;
      to: string;
      sourceHandle?: 'left' | 'right' | 'top' | 'bottom';
      targetHandle?: 'left' | 'right' | 'top' | 'bottom';
    }
  | { type: 'graph.delete'; workflowId: string; nodeIds: string[]; edgeIds: string[] }
  | {
      type: 'node.change';
      workflowId: string;
      nodeId: string;
      kind: InsertKind;
      calleeId?: string;
      newWorkflowName?: string;
    }
  | { type: 'node.put'; workflowId: string; node: WorkflowNode }
  | { type: 'node.delete'; workflowId: string; nodeId: string }
  | { type: 'group.update'; group: ScenarioDocument['groups'][number] };
export function skeleton(id: string, name: string): WorkflowDefinition {
  return {
    id,
    name,
    inputs: {},
    outputs: {},
    nodes: [
      { id: 'start', type: 'start' },
      { id: 'end', type: 'end' },
    ],
    edges: [{ id: 'edge-1', from: 'start', to: 'end' }],
    layout: { positions: { start: { x: 80, y: 160 }, end: { x: 340, y: 160 } } },
  };
}
export function newDraft(): ScenarioDocument {
  return {
    schemaVersion: 2,
    id: `scenario-${crypto.randomUUID()}`,
    name: 'Untitled scenario',
    environment: { variables: {} },
    clientProfiles: [{ id: 'client-1', baseUrl: 'http://localhost:8080', timeoutMs: 5000, headers: {} }],
    identities: [],
    assertions: [],
    groups: [
      {
        id: 'group-1',
        clientProfileId: 'client-1',
        identityAssignment: { mode: 'shared', identityId: null },
        executor: { kind: 'once' },
        lifecycle: {
          initialize: null,
          iteration: { workflowId: 'workflow-1', inputs: {} },
          cleanup: null,
          cleanupTimeoutMs: 2000,
        },
      },
    ],
    workflows: [skeleton('workflow-1', 'Iteration')],
  };
}
export function executeCommand(document: ScenarioDocument, command: EditorCommand): ScenarioDocument {
  const next = structuredClone(document);
  if (command.type === 'phase.create' || command.type === 'phase.detach') {
    if (command.type === 'phase.detach') detachPhase(next, command.phase);
    else ensurePhase(next, command.phase);
    return documentSchema.parse(next);
  }
  if (command.type === 'phase.move') {
    moveToPhase(next, command.from, command.to, command.nodeIds);
    if (command.position && command.nodeIds.length === 1) {
      const workflow = ensurePhase(next, command.to);
      if (![command.position.x, command.position.y].every(Number.isFinite)) throw new Error('Invalid node position.');
      workflow.layout ??= { positions: {} };
      workflow.layout.positions[command.nodeIds[0]] = {
        x: Math.round(command.position.x * 100) / 100,
        y: Math.round(command.position.y * 100) / 100,
      };
    }
    return documentSchema.parse(next);
  }
  if (command.type === 'phase.node.add') {
    let calleeId = command.calleeId;
    if (command.kind === 'call' && command.newWorkflowName) {
      const flow = skeleton(
        allocateId(
          'workflow',
          next.workflows.map((flow) => flow.id)
        ),
        command.newWorkflowName.trim() || 'Workflow'
      );
      flow.edges = [];
      next.workflows.push(flow);
      calleeId = flow.id;
    }
    addPhaseNode(next, command.phase, command.kind, calleeId);
    return documentSchema.parse(next);
  }
  if (command.type === 'node.move') {
    const workflow = next.workflows.find((workflow) => workflow.id === command.workflowId);
    if (!workflow?.nodes.some((node) => node.id === command.nodeId)) throw new Error('Node no longer exists.');
    const { x, y } = command.position;
    if (!Number.isFinite(x) || !Number.isFinite(y)) throw new Error('Enter finite coordinates.');
    workflow.layout = {
      ...workflow.layout,
      positions: {
        ...workflow.layout?.positions,
        [command.nodeId]: { x: Number(x.toFixed(2)), y: Number(y.toFixed(2)) },
      },
    };
    return documentSchema.parse(next);
  }

  if (command.type === 'scenario.configure')
    return documentSchema.parse(configureScenario(next, command.configuration));
  if (command.type === 'workflow.configure')
    return documentSchema.parse(configureWorkflow(next, command.workflowId, command.configuration));
  if (command.type === 'name') next.name = command.name;
  else if (command.type === 'baseUrl') {
    const group = next.groups[0];
    const profile = next.clientProfiles.find((profile) => profile.id === group?.clientProfileId);
    if (!profile) throw new Error('Select a client profile first.');
    profile.baseUrl = command.baseUrl;
  } else if (command.type === 'group.update') {
    const index = next.groups.findIndex((g) => g.id === command.group.id);
    if (index < 0) throw new Error('Unknown workload group.');
    next.groups[index] = command.group;
  } else if (command.type === 'workflow.create')
    next.workflows.push(
      skeleton(
        allocateId(
          'workflow',
          next.workflows.map((w) => w.id)
        ),
        command.name
      )
    );
  else {
    const workflow = next.workflows.find((w) => w.id === command.workflowId);
    if (!workflow) throw new Error('Unknown workflow.');
    if (command.type === 'authentication.insert') insertAuthentication(next, command.workflowId, command.edgeId);
    if (command.type === 'authentication.configure')
      configureAuthentication(next, command.workflowId, command.nodeId, command.settings);
    if (command.type === 'http.insert') insertNode(next, workflow, 'http', command);
    if (
      command.type === 'node.change' &&
      workflow.nodes.find((node) => node.id === command.nodeId)?.type === command.kind
    )
      return documentSchema.parse(next);
    if (command.type === 'node.insert' || command.type === 'node.add' || command.type === 'node.change') {
      let calleeId = command.calleeId;
      if (command.kind === 'call' && command.newWorkflowName) {
        calleeId = allocateId(
          'workflow',
          next.workflows.map((workflow) => workflow.id)
        );
        const created = skeleton(calleeId, command.newWorkflowName.trim() || 'Workflow');
        if (command.type === 'node.add' || command.type === 'node.change') created.edges = [];
        next.workflows.push(created);
      }
      if (command.type === 'node.change') changeNode(next, workflow, command.nodeId, command.kind, calleeId);
      else if (command.type === 'node.add') addNode(next, workflow, command.kind, command.afterNodeId, calleeId);
      else insertNode(next, workflow, command.kind, command, calleeId);
    }
    if (command.type === 'gateway.configure') {
      const node = workflow.nodes.find((node) => node.id === command.nodeId);
      if (node?.type !== 'exclusiveSplit') throw new Error('Select an exclusive split.');
      const current = workflow.edges.filter((edge) => edge.from === node.id);
      if (
        current.length !== command.routes.length ||
        current.some(
          (edge) =>
            !command.routes.some((route) => route.id === edge.id && route.from === edge.from && route.to === edge.to)
        )
      )
        throw new Error('Connections changed; reopen route configuration.');
      if (command.inputs) node.config.inputs = command.inputs;
      node.config.sourceNodeId = command.sourceNodeId;
      workflow.edges = workflow.edges.map((edge) => command.routes.find((route) => route.id === edge.id) ?? edge);
    }
    if (command.type === 'boundary.add') {
      if (workflow.nodes.some((node) => node.type === command.kind))
        throw new Error(`This workflow already has a ${command.kind} node.`);
      const id = allocateId(
        command.kind,
        workflow.nodes.map((node) => node.id)
      );
      workflow.nodes.push({ id, type: command.kind });
      const positions = { ...workflow.layout?.positions };
      const x =
        command.kind === 'start' ? 80 : Math.max(80, ...Object.values(positions).map((position) => position.x)) + 260;
      positions[id] = { x, y: 160 };
      workflow.layout = { ...workflow.layout, positions };
    }
    if (command.type === 'edge.connect' || command.type === 'edge.reconnect')
      connect(
        workflow,
        command.from,
        command.to,
        command.type === 'edge.reconnect' ? command.edgeId : undefined,
        command.sourceHandle,
        command.targetHandle
      );
    if (command.type === 'graph.delete') deleteGraphElements(workflow, command.nodeIds, command.edgeIds);
    if (command.type === 'node.rename') {
      renameNode(workflow, command.nodeId, command.nextId);
      if (command.name !== undefined) {
        const node = workflow.nodes.find((node) => node.id === command.nextId)!;
        if (command.name.trim()) node.name = command.name;
        else delete node.name;
      }
    }
    if (command.type === 'workflow.duplicate')
      next.workflows.push({
        ...structuredClone(workflow),
        id: allocateId(
          'workflow',
          next.workflows.map((w) => w.id)
        ),
        name: `${workflow.name} copy`,
      });
    if (command.type === 'workflow.delete') {
      if (
        next.groups.some((g) =>
          Object.values(g.lifecycle).some((b) => typeof b === 'object' && b?.workflowId === workflow.id)
        ) ||
        next.workflows.some((w) => w.nodes.some((n) => n.type === 'call' && n.config.workflowId === workflow.id))
      )
        throw new Error('Workflow is referenced by a call or workload. Remove its bindings first.');
      next.workflows = next.workflows.filter((w) => w.id !== workflow.id);
    }
    if (command.type === 'workflow.update') {
      if (command.value.id !== workflow.id) throw new Error('Stable workflow IDs cannot be renamed.');
      next.workflows[next.workflows.indexOf(workflow)] = command.value;
    }
    if (command.type === 'node.put') {
      const index = workflow.nodes.findIndex((n) => n.id === command.node.id);
      const before = workflow.nodes[index];
      if (before?.type === 'extract' && command.node.type === 'extract')
        checkExtractionOutputs(workflow, before.id, before.config, command.node.config);
      if (index < 0) workflow.nodes.push(command.node);
      else workflow.nodes[index] = command.node;
    }
    if (command.type === 'node.delete') deleteGraphElements(workflow, [command.nodeId], []);
  }
  return documentSchema.parse(next);
}
