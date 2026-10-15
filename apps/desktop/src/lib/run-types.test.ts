import { emptyEventContext } from './run-types';
import { describe, expect, it } from 'vitest';
import { eventSchema, reportSchema, runMessageSchema } from './run-types';
const event = {
  runId: 'run',
  sequence: 1,
  context: emptyEventContext,
  elapsedNs: 0,
  userId: 'user:1',
  nodeId: 'apply-token',
  type: 'session.updated',
  payload: { setHeaders: ['authorization'], removedHeaders: [] },
};
const report = {
  reportVersion: 3,
  graph: { groups: [], workflows: [] },
  cleanup: { status: 'notConfigured', error: null },
  runId: 'run',
  status: 'passed',
  snapshot: {},
  summary: { requests: 2, responses: 2, httpErrorResponses: 0, elapsedNs: 0 },
  events: [event],
  error: null,
  eventsTruncated: false,
  deliveryDroppedEvents: 0,
};
describe('M3.5 report wire contract', () => {
  it('accepts version 3 session metadata in live messages and reports', () => {
    expect(eventSchema.parse(event)).toEqual(event);
    expect(runMessageSchema.parse({ protocolVersion: 3, kind: 'event', event })).toEqual({
      protocolVersion: 3,
      kind: 'event',
      event,
    });
    expect(runMessageSchema.parse({ protocolVersion: 3, kind: 'finished', report })).toEqual({
      protocolVersion: 3,
      kind: 'finished',
      report,
    });
  });
  it('rejects historical and unknown report versions', () => {
    for (const reportVersion of [1, 2, 4, '3'])
      expect(reportSchema.safeParse({ ...report, reportVersion }).success).toBe(false);
  });
  it('rejects session payloads containing values or malformed names', () => {
    for (const payload of [
      { ...event.payload, values: { authorization: 'private-marker' } },
      { setHeaders: { authorization: 'private-marker' }, removedHeaders: [] },
      { setHeaders: [42], removedHeaders: [] },
    ]) {
      expect(eventSchema.safeParse({ ...event, payload }).success).toBe(false);
    }
  });
});

it('requires a versioned live wrapper and invocation context', () => {
  expect(runMessageSchema.safeParse({ kind: 'event', event }).success).toBe(false);
  expect(runMessageSchema.safeParse({ protocolVersion: 2, kind: 'event', event }).success).toBe(false);
  const { context, ...missing } = event;
  expect(context).toBeDefined();
  expect(eventSchema.safeParse(missing).success).toBe(false);
});
it('rejects values injected into the safe graph projection', () => {
  const graph = {
    groups: [],
    workflows: [
      {
        key: 'wf:0',
        id: 'main',
        name: 'Main',
        nodes: [{ key: 'wf:0/node:0', id: 'request', type: 'http', position: { x: 0, y: 0 }, body: 'private-marker' }],
        edges: [],
      },
    ],
  };
  expect(reportSchema.safeParse({ ...report, graph }).success).toBe(false);
});
