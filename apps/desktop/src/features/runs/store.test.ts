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

it('filters early callbacks against the acknowledged run ID and never revives a terminal task', async () => {
  const { store, bridge, callbacks } = setup();
  bridge.start.mockImplementationOnce(async (_source, receive) => {
    callbacks.push(receive);
    receive({ protocolVersion: 3, kind: 'event', event: event('wrong') });
    receive({ protocolVersion: 3, kind: 'event', event: event('first') });
    return { runId: 'first' };
  });
  await store.getState().start(exampleDocument);
  expect(store.getState().events.map((event) => event.runId)).toEqual(['first']);
  callbacks[0]({ protocolVersion: 3, kind: 'finished', report: report() });
  callbacks[0]({ protocolVersion: 3, kind: 'failed', message: 'late failure' });
  expect(store.getState().status).toBe('passed');
});

it('retains snapshot navigation and expansion through final reconciliation and resets them for a new run', async () => {
  const { store, callbacks } = setup();
  await store.getState().start(exampleDocument);
  const first = event();
  callbacks[0]({ protocolVersion: 3, kind: 'event', event: first });
  store.getState().toggleExecution('node');
  store.getState().showSnapshot(true);
  store.setState({ selectedWorkflow: 'workflow', selectedNode: 'node' });
  callbacks[0]({ protocolVersion: 3, kind: 'finished', report: { ...report(), deliveryDroppedEvents: 2 } });
  expect(store.getState()).toMatchObject({
    showRun: true,
    expanded: { node: true },
    selectedWorkflow: 'workflow',
    selectedNode: 'node',
  });
  await store.getState().start(exampleDocument);
  expect(store.getState()).toMatchObject({ expanded: {}, selectedWorkflow: null, selectedNode: null });
});

it('sorts and deduplicates live history and reconciles only the retained report events', async () => {
  const { store, callbacks } = setup();
  await store.getState().start(exampleDocument);
  const later = { ...event(), sequence: 3 };
  callbacks[0]({ protocolVersion: 3, kind: 'event', event: later });
  callbacks[0]({ protocolVersion: 3, kind: 'event', event: event() });
  callbacks[0]({ protocolVersion: 3, kind: 'event', event: later });
  expect(store.getState().events.map((event) => event.sequence)).toEqual([1, 3]);
  const complete = [event(), { ...event(), sequence: 2 }, later];
  callbacks[0]({
    protocolVersion: 3,
    kind: 'finished',
    report: { ...report(), events: complete, deliveryDroppedEvents: 1 },
  });
  expect(store.getState().events).toEqual(complete);
});

it('shows the final snapshot when run.started was not delivered and respects an explicit return to the scenario', async () => {
  const { store, callbacks } = setup();
  await store.getState().start(exampleDocument);
  callbacks[0]({
    protocolVersion: 3,
    kind: 'event',
    event: { ...event(), sequence: 2, type: 'user.spawned', payload: {} },
  });
  expect(store.getState().showRun).toBe(false);
  callbacks[0]({ protocolVersion: 3, kind: 'finished', report: report() });
  expect(store.getState().showRun).toBe(true);
  await store.getState().start(exampleDocument);
  store.getState().showSnapshot(false);
  callbacks[1]({ protocolVersion: 3, kind: 'finished', report: report('second') });
  expect(store.getState().showRun).toBe(false);
});
