import type { RunEvent, RunReport } from '../../lib/run-types';

export type RunGraph = RunReport['graph'];
export type Progress = 'pending' | 'active' | 'completed' | 'failed' | 'cancelled' | 'skipped' | 'unknown';
export type NodeExecution = {
  key: string;
  nodeKey: string;
  nodeId: string;
  state: Progress;
  events: RunEvent[];
  started: number | null;
  finished: number | null;
  httpStatus?: number;
  error?: string;
  route?: string;
};
export type Invocation = {
  key: string;
  id: string | null;
  workflowKey: string | null;
  parentId: string | null;
  callerNodeKey: string | null;
  groupKey: string | null;
  userId: string | null;
  phase: RunEvent['context']['phase'];
  iteration: number | null;
  state: Progress;
  latestEdge?: string;
  nodes: NodeExecution[];
  traversed: Set<string>;
  skippedNodes: Set<string>;
  skippedEdges: Set<string>;
};
export type Projection = {
  graph: RunGraph | null;
  invocations: Invocation[];
  otherEvents: RunEvent[];
  phases: Partial<Record<'initialize' | 'iteration' | 'cleanup', Progress>>;
  incomplete: boolean;
};
const terminal = (status: RunReport['status']): Progress =>
  status === 'passed' ? 'completed' : status === 'cancelled' ? 'cancelled' : 'failed';
export function invocationKey(event: RunEvent): string {
  const c = event.context;
  return JSON.stringify([
    event.runId,
    c.groupKey,
    event.userId,
    c.phase,
    c.iteration,
    c.workflowKey,
    c.invocationId,
    c.parentInvocationId,
    c.callerNodeKey,
  ]);
}
function reachable(graph: RunGraph['workflows'][number], start: string): Set<string> {
  const seen = new Set<string>();
  const queue = [start];
  while (queue.length) {
    const key = queue.pop()!;
    if (seen.has(key)) continue;
    seen.add(key);
    graph.edges.filter((edge) => edge.fromNodeKey === key).forEach((edge) => queue.push(edge.toNodeKey));
  }
  return seen;
}
/** Project only observed facts; identifiers are scoped by the entire invocation context. */
export function projectRun(
  events: RunEvent[],
  report: RunReport | null = null,
  taskStatus?: RunReport['status']
): Projection {
  const runId = report?.runId ?? events.find((event) => event.type === 'run.started')?.runId ?? events[0]?.runId;
  const ordered = [
    ...new Map(events.filter((event) => event.runId === runId).map((event) => [event.sequence, event])).values(),
  ].sort((a, b) => a.sequence - b.sequence);
  const started = ordered.find((event) => event.type === 'run.started');
  const graph = report?.graph ?? started?.payload.graph ?? null;
  const result: Projection = {
    graph,
    invocations: [],
    otherEvents: [],
    phases: {},
    incomplete: Boolean(report?.eventsTruncated),
  };
  if (!started && report) result.incomplete = true;
  const invocations = new Map<string, Invocation>();
  let previous = 0;
  for (const event of ordered) {
    if (event.sequence !== previous + 1) result.incomplete = true;
    previous = event.sequence;
    if (event.type === 'lifecycle.started') result.phases[event.payload.phase] = 'active';
    if (event.type === 'lifecycle.finished') result.phases[event.payload.phase] = terminal(event.payload.status);
    if (event.type === 'cleanup.skipped') result.phases.cleanup = 'skipped';
    if (!event.context.invocationId && !event.nodeId && !event.context.nodeKey) {
      result.otherEvents.push(event);
      continue;
    }
    const key = invocationKey(event);
    let invocation = invocations.get(key);
    if (!invocation) {
      const c = event.context;
      invocation = {
        key,
        id: c.invocationId,
        workflowKey: c.workflowKey,
        parentId: c.parentInvocationId,
        callerNodeKey: c.callerNodeKey,
        groupKey: c.groupKey,
        userId: event.userId,
        phase: c.phase,
        iteration: c.iteration,
        state: 'unknown',
        nodes: [],
        traversed: new Set(),
        skippedNodes: new Set(),
        skippedEdges: new Set(),
      };
      invocations.set(key, invocation);
      result.invocations.push(invocation);
    }
    if (event.type === 'workflow.started') {
      invocation.state = 'active';
      continue;
    }
    if (event.type === 'workflow.finished') {
      invocation.state = terminal(event.payload.status);
      continue;
    }
    if (event.type === 'edge.traversed') {
      if (event.context.edgeKey) {
        invocation.traversed.add(event.context.edgeKey);
        invocation.latestEdge = event.context.edgeKey;
      }
      continue;
    }
    const nodeKey = event.context.nodeKey ?? `uncorrelated:${event.nodeId}`;
    if (!event.nodeId && !event.context.nodeKey) continue;
    let node = [...invocation.nodes].reverse().find((node) => node.nodeKey === nodeKey);
    if (!node || (event.type === 'step.started' && node.started !== null)) {
      const count = invocation.nodes.filter((node) => node.nodeKey === nodeKey).length;
      node = {
        key: `${key}/${nodeKey}/${count}`,
        nodeKey,
        nodeId: event.nodeId ?? 'Unknown node',
        state: 'unknown',
        events: [],
        started: null,
        finished: null,
      };
      invocation.nodes.push(node);
    }
    node.events.push(event);
    if (event.type === 'step.started') {
      node.started = event.elapsedNs;
      node.state = 'active';
      if (invocation.state === 'unknown') invocation.state = 'active';
    }
    if (event.type === 'step.finished') {
      node.finished = event.elapsedNs;
      node.state = 'completed';
    }
    if (event.type === 'step.failed') {
      node.finished = event.elapsedNs;
      node.state = event.payload.error.kind === 'cancelled' ? 'cancelled' : 'failed';
      node.error = `${event.payload.error.kind}: ${event.payload.error.message}`;
    }
    if (event.type === 'response.received') node.httpStatus = event.payload.status;
    if (event.type === 'route.selected') {
      const workflow = graph?.workflows.find((workflow) => workflow.key === invocation!.workflowKey);
      const chosen = workflow?.edges.find((edge) => edge.key === event.payload.selectedEdgeKey);
      const target = workflow?.nodes.find((node) => node.key === chosen?.toNodeKey);
      node.route = `Selected route: ${target?.name ?? target?.id ?? 'Unknown'}`;
      if (workflow && chosen) {
        const selected = reachable(workflow, chosen.toNodeKey);
        for (const edgeKey of event.payload.unselectedEdgeKeys) {
          invocation.skippedEdges.add(edgeKey);
          const edge = workflow.edges.find((edge) => edge.key === edgeKey);
          if (edge)
            reachable(workflow, edge.toNodeKey).forEach((key) => {
              if (!selected.has(key)) invocation!.skippedNodes.add(key);
            });
        }
        workflow.edges.forEach((edge) => {
          if (invocation!.skippedNodes.has(edge.fromNodeKey)) invocation!.skippedEdges.add(edge.key);
        });
      }
    }
  }
  const finalEvent = ordered.find((event) => event.type === 'run.finished');
  const status =
    report?.status ??
    finalEvent?.payload.status ??
    (ordered.some((event) => event.type === 'run.cancelled') ? 'cancelled' : undefined) ??
    taskStatus;
  if (taskStatus && !report && !finalEvent && ordered.length) result.incomplete = true;
  if (report && report.cleanup.status !== 'notConfigured')
    result.phases.cleanup =
      report.cleanup.status === 'completed'
        ? 'completed'
        : report.cleanup.status === 'skipped'
          ? 'skipped'
          : report.cleanup.status === 'cancelled'
            ? 'cancelled'
            : 'failed';
  if (status)
    for (const phase of ['initialize', 'iteration', 'cleanup'] as const)
      if (result.phases[phase] === 'active') result.phases[phase] = status === 'cancelled' ? 'cancelled' : 'unknown';
  for (const invocation of result.invocations) {
    if (invocation.state === 'active' && status) invocation.state = status === 'cancelled' ? 'cancelled' : 'unknown';
    for (const node of invocation.nodes) {
      if (node.state === 'active' && (status || invocation.state !== 'active'))
        node.state = invocation.state === 'cancelled' || status === 'cancelled' ? 'cancelled' : 'unknown';
      if (node.started === null || (node.finished !== null && node.finished < (node.started ?? 0)))
        result.incomplete = true;
      invocation.skippedNodes.delete(node.nodeKey);
    }
  }
  return result;
}
export function nodeProgress(invocation: Invocation | undefined, nodeKey: string): Progress {
  return (
    [...(invocation?.nodes ?? [])].reverse().find((node) => node.nodeKey === nodeKey)?.state ??
    (invocation?.skippedNodes.has(nodeKey) ? 'skipped' : invocation?.state === 'active' ? 'pending' : 'unknown')
  );
}
export function duration(node: NodeExecution): string {
  return node.started !== null && node.finished !== null && node.finished >= node.started
    ? `${((node.finished - node.started) / 1e6).toFixed(1)} ms`
    : '—';
}

/** Default node inspection to useful safe observations, while honoring explicit event selection. */
export function detailEvent(node: NodeExecution, sequence?: number | null): RunEvent | undefined {
  const selected = node.events.find((event) => event.sequence === sequence);
  if (selected) return selected;
  const latest = [...node.events].reverse();
  for (const type of ['step.failed', 'response.received', 'session.updated', 'route.selected', 'request.sent']) {
    const event = latest.find((event) => event.type === type);
    if (event) return event;
  }
  return latest[0];
}
