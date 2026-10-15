import { emptyEventContext } from '../../lib/run-types';
import { describe, expect, it, vi } from 'vitest';
import { createRunStore } from './store';
import { exampleDocument } from '../scenarios/store';
import type { RunEvent, RunMessage, RunReport } from '../../lib/run-types';
const event = (id = 'first'): RunEvent => ({
  runId: id,
  sequence: 1,
  context: emptyEventContext,
  elapsedNs: 10,
  userId: 'visitors:1',
  nodeId: null,
  type: 'run.started',
  payload: { scenarioId: 'hello-api', graph: { groups: [], workflows: [] } },
});
const report = (id = 'first'): RunReport => ({
  reportVersion: 3,
  graph: { groups: [], workflows: [] },
  cleanup: { status: 'notConfigured', error: null },
  runId: id,
  status: 'passed',
  snapshot: {},
  summary: { requests: 1, responses: 1, httpErrorResponses: 0, elapsedNs: 10 },
  events: [event(id)],
  error: null,
  eventsTruncated: false,
  deliveryDroppedEvents: 0,
});
function setup() {
  const callbacks: ((message: RunMessage) => void)[] = [];
  const bridge = {
    start: vi.fn(async (_source: string, receive: (message: RunMessage) => void) => {
      callbacks.push(receive);
      return { runId: callbacks.length === 1 ? 'first' : 'second' };
    }),
    cancel: vi.fn().mockResolvedValue(true),
    save: vi.fn().mockResolvedValue(true),
  };
  return { bridge, callbacks, store: createRunStore(bridge) };
}
describe('run lifecycle', () => {
  it('captures the scenario before IPC and keeps active input independent of edits', async () => {
    const { store, bridge } = setup();
    const document = structuredClone(exampleDocument);
    const start = store.getState().start(document);
    document.clientProfiles[0].baseUrl = 'http://changed.invalid';
    await start;
    expect(JSON.parse(bridge.start.mock.calls[0][0]).clientProfiles[0].baseUrl).toBe(
      exampleDocument.clientProfiles[0].baseUrl
    );
    await store.getState().start(document);
    expect(bridge.start).toHaveBeenCalledTimes(1);
  });
  it('keeps an early completion when the start command resolves later', async () => {
    const { store, bridge } = setup();
    bridge.start.mockImplementationOnce(async (_source, receive) => {
      receive({ protocolVersion: 3, kind: 'finished', report: report() });
      return { runId: 'first' };
    });
    await store.getState().start(exampleDocument);
    expect(store.getState().status).toBe('passed');
    expect(store.getState().report).not.toBeNull();
  });
  it('waits for the final cancelled report and resynchronizes dropped live events', async () => {
    const { store, callbacks, bridge } = setup();
    await store.getState().start(exampleDocument);
    callbacks[0]({ protocolVersion: 3, kind: 'event', event: event() });
    await store.getState().cancel();
    expect(bridge.cancel).toHaveBeenCalledWith('first');
    expect(store.getState().status).toBe('cancelling');
    callbacks[0]({
      protocolVersion: 3,
      kind: 'finished',
      report: { ...report(), status: 'cancelled', deliveryDroppedEvents: 2 },
    });
    expect(store.getState().status).toBe('cancelled');
    expect(store.getState().events).toEqual(report().events);
  });
  it('ignores stale callbacks from a previous run and rejects mismatched run IDs', async () => {
    const { store, callbacks } = setup();
    await store.getState().start(exampleDocument);
    callbacks[0]({ protocolVersion: 3, kind: 'finished', report: report() });
    await store.getState().start(exampleDocument);
    callbacks[0]({ protocolVersion: 3, kind: 'finished', report: report() });
    callbacks[1]({ protocolVersion: 3, kind: 'event', event: event('wrong') });
    expect(store.getState().report).toBeNull();
    expect(store.getState().events).toEqual([]);
    callbacks[1]({ protocolVersion: 3, kind: 'event', event: event('second') });
    expect(store.getState().events).toHaveLength(1);
  });
  it('reports a cancellation bridge failure while keeping the run active', async () => {
    const { store, bridge } = setup();
    bridge.cancel.mockRejectedValue(new Error('offline'));
    await store.getState().start(exampleDocument);
    await store.getState().cancel();
    expect(store.getState().status).toBe('running');
    expect(store.getState().error).toContain('Try again');
  });
  it('does not revive a failed task when start resolves late', async () => {
    const { store, bridge } = setup();
    bridge.start.mockImplementationOnce(async (_source, receive) => {
      receive({ protocolVersion: 3, kind: 'failed', message: 'Execution task failed' });
      return { runId: 'first' };
    });
    await store.getState().start(exampleDocument);
    expect(store.getState().status).toBe('error');
  });
  it('shows missing-secret diagnostics and allows retry after rejected preparation', async () => {
    const { store, bridge } = setup();
    bridge.start.mockRejectedValueOnce([
      {
        category: 'validation',
        code: 'missingSecret',
        message: 'Required environment reference: INTERL_ALICE_PASSWORD.',
        fieldPath: '/identities',
        nodeId: null,
        filePath: null,
        line: null,
        column: null,
      },
    ]);
    await store.getState().start(exampleDocument);
    expect(store.getState().status).toBe('error');
    expect(store.getState().error).toBe('missingSecret: Required environment reference: INTERL_ALICE_PASSWORD.');
    expect(store.getState().runId).toBeNull();
    await store.getState().start(exampleDocument);
    expect(bridge.start).toHaveBeenCalledTimes(2);
    expect(store.getState().status).toBe('running');
    expect(store.getState().error).toBeNull();
  });
  it('uses an auth-aware fallback without exposing raw start errors', async () => {
    const { store, bridge } = setup();
    bridge.start.mockRejectedValueOnce(new Error('private-marker'));
    await store.getState().start(exampleDocument);
    expect(store.getState().error).toContain('required environment references');
    expect(store.getState().error).not.toContain('private-marker');
    expect(store.getState().error).not.toContain('M2');
  });
});
