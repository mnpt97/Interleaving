import type { RunEvent, RunReport } from '../../lib/run-types';
import { emptyEventContext } from '../../lib/run-types';
import type { RunGraph } from './projection';

// Safe, deterministic event catalogue used by projection and component regressions.
export const runGraph: RunGraph = {
  groups: [
    {
      key: 'group',
      id: 'Visitors',
      initialize: null,
      inlinePhases: ['iteration'],
      iteration: 'main',
      cleanup: 'child',
    },
  ],
  workflows: [
    {
      key: 'main',
      id: 'main',
      name: 'Journey',
      nodes: [
        { key: 's', id: 'start', type: 'start', position: { x: 80, y: 160 } },
        { key: 'h', id: 'health', name: 'Check health', type: 'http', position: { x: 340, y: 160 } },
        { key: 'x', id: 'choice', type: 'exclusiveSplit', position: { x: 600, y: 160 } },
        { key: 'a', id: 'accepted', name: 'Healthy route', type: 'call', position: { x: 860, y: 60 } },
        { key: 'b', id: 'fallback', type: 'http', position: { x: 860, y: 260 } },
        { key: 'm', id: 'merged', type: 'exclusiveMerge', position: { x: 1120, y: 160 } },
        { key: 'e', id: 'end', type: 'end', position: { x: 1380, y: 160 } },
      ],
      edges: [
        { key: 'sh', id: 'sh', fromNodeKey: 's', toNodeKey: 'h', sourceHandle: 'top', targetHandle: 'left' },
        { key: 'hx', id: 'hx', fromNodeKey: 'h', toNodeKey: 'x' },
        { key: 'xa', id: 'xa', fromNodeKey: 'x', toNodeKey: 'a' },
        { key: 'xb', id: 'xb', fromNodeKey: 'x', toNodeKey: 'b' },
        { key: 'am', id: 'am', fromNodeKey: 'a', toNodeKey: 'm' },
        { key: 'bm', id: 'bm', fromNodeKey: 'b', toNodeKey: 'm' },
        { key: 'me', id: 'me', fromNodeKey: 'm', toNodeKey: 'e' },
      ],
    },
    {
      key: 'child',
      id: 'inspect',
      name: 'Inspect',
      nodes: [{ key: 'ch', id: 'health', name: 'Child health', type: 'http', position: { x: 80, y: 160 } }],
      edges: [],
    },
  ],
};
export function observation(
  type: RunEvent['type'],
  sequence: number,
  payload: unknown = {},
  context: Partial<RunEvent['context']> = {},
  nodeId: string | null = 'health'
): RunEvent {
  return {
    runId: 'run',
    sequence,
    elapsedNs: sequence * 1e6,
    userId: 'user:1',
    nodeId,
    context: {
      ...emptyEventContext,
      groupKey: 'group',
      phase: 'iteration',
      iteration: 1,
      workflowKey: 'main',
      invocationId: 'root',
      nodeKey: nodeId ? 'h' : null,
      ...context,
    },
    type,
    payload,
  } as RunEvent;
}
export const begin = () =>
  observation(
    'run.started',
    1,
    { scenarioId: 'hello-api', graph: runGraph },
    { workflowKey: null, invocationId: null, nodeKey: null },
    null
  );
export function completeReport(events: RunEvent[]): RunReport {
  return {
    reportVersion: 3,
    runId: 'run',
    graph: runGraph,
    cleanup: { status: 'notConfigured', error: null },
    status: 'passed',
    snapshot: {},
    summary: { requests: 1, responses: 1, httpErrorResponses: 0, elapsedNs: 20e6 },
    events,
    error: null,
    eventsTruncated: false,
    deliveryDroppedEvents: 0,
  };
}
export const response = (sequence: number) =>
  observation('response.received', sequence, {
    status: 200,
    headers: {},
    body: {
      value: null,
      bytes: 0,
      truncated: false,
      omitted: true,
      reason: 'Sensitive authentication payload omitted',
    },
  });
