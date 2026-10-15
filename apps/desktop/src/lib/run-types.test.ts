import { describe, expect, it } from 'vitest';
import { eventSchema, reportSchema, runMessageSchema } from './run-types';
const event = {
  runId: 'run',
  sequence: 1,
  elapsedNs: 0,
  userId: 'user:1',
  nodeId: 'apply-token',
  type: 'session.updated',
  payload: { setHeaders: ['authorization'], removedHeaders: [] },
};
const report = {
  reportVersion: 2,
  runId: 'run',
  status: 'passed',
  snapshot: {},
  summary: { requests: 2, responses: 2, httpErrorResponses: 0, elapsedNs: 0 },
  events: [event],
  error: null,
  eventsTruncated: false,
  deliveryDroppedEvents: 0,
};
describe('M3 report wire contract', () => {
  it('accepts version 2 session metadata in live messages and reports', () => {
    expect(eventSchema.parse(event)).toEqual(event);
    expect(runMessageSchema.parse({ kind: 'event', event })).toEqual({ kind: 'event', event });
    expect(runMessageSchema.parse({ kind: 'finished', report })).toEqual({ kind: 'finished', report });
  });
  it('rejects historical and unknown report versions', () => {
    for (const reportVersion of [1, 3, '2'])
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
