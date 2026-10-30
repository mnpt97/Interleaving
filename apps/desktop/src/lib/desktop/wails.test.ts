import { afterEach, expect, it, vi } from 'vitest';
import { wailsHost, probeBridge, streamProbe } from './wails';
import { emptyEventContext } from '../run-types';
import { exampleDocument } from '../../features/scenarios/store';
import { createRunStore } from '../../features/runs/store';
const fake = vi.hoisted(() => ({
  ready: vi.fn().mockResolvedValue(undefined),
  close: vi.fn().mockResolvedValue(undefined),
  probe: vi.fn(),
  stream: vi.fn(),
  stop: vi.fn(),
  validate: vi.fn(),
  read: vi.fn(),
  save: vi.fn(),
  cancel: vi.fn().mockResolvedValue(true),
  report: vi.fn(),
  openDialog: vi.fn(),
  saveDialog: vi.fn(),
  sockets: [] as {
    onopen: null | (() => void);
    onmessage: null | ((event: { data: unknown }) => void);
    onerror: null | (() => void);
    onclose: null | (() => void);
    send: ReturnType<typeof vi.fn>;
    close: ReturnType<typeof vi.fn>;
  }[],
  handlers: new Map<string, (event: { data: unknown }) => unknown>(),
}));
vi.mock('./generated/interleaving/internal/adapters/inbound/wails/service', () => ({
  Ready: fake.ready,
  Close: fake.close,
  Probe: fake.probe,
  StreamProbe: fake.stream,
  ValidateSource: fake.validate,
  ReadScenario: fake.read,
  SaveScenarioJSON: fake.save,
  CancelScenario: fake.cancel,
  SaveRunReport: fake.report,
  OpenDialog: fake.openDialog,
  SaveDialog: fake.saveDialog,
  SaveReportDialog: fake.saveDialog,
}));
vi.mock('@wailsio/runtime', () => ({
  JSONStream: () => {
    const socket = { onopen: null, onmessage: null, onerror: null, onclose: null, send: vi.fn(), close: vi.fn() };
    fake.sockets.push(socket);
    return socket;
  },
  Events: {
    On: (name: string, callback: (event: { data: unknown }) => unknown) => {
      fake.handlers.set(name, callback);
      return fake.stop;
    },
  },
}));
afterEach(() => {
  vi.clearAllMocks();
  fake.handlers.clear();
  fake.sockets.length = 0;
});
const event = (sequence: number) => ({ id: 'probe-1', sequence, at: '2026-10-28T00:00:00Z', synthetic: true });
const runEvent = (runId = 'run') => ({
  runId,
  sequence: 1,
  context: emptyEventContext,
  elapsedNs: 0,
  userId: 'vu:1',
  nodeId: null,
  type: 'run.started',
  payload: { scenarioId: 'hello-api', graph: { groups: [], workflows: [] } },
});
const runReport = (runId = 'run') => ({
  reportVersion: 3,
  runId,
  status: 'passed',
  graph: { groups: [], workflows: [] },
  cleanup: { status: 'notConfigured', error: null },
  snapshot: {},
  summary: { requests: 1, responses: 1, httpErrorResponses: 0, elapsedNs: 0 },
  events: [runEvent(runId)],
  error: null,
  eventsTruncated: false,
  deliveryDroppedEvents: 0,
});
async function socketReady() {
  await vi.waitFor(() => expect(fake.sockets).toHaveLength(1));
  const socket = fake.sockets[0];
  socket.onopen!();
  return socket;
}
const frame = (socket: (typeof fake.sockets)[number], data: unknown) => socket.onmessage!({ data });
it('advertises authoritative scenario services and validates their boundary', async () => {
  expect(wailsHost.available).toBe(true);
  fake.validate.mockResolvedValue({ valid: true, document: exampleDocument, diagnostics: [] });
  expect((await wailsHost.validateSource('{}', 'json')).valid).toBe(true);
  fake.validate.mockResolvedValue({ valid: true });
  await expect(wailsHost.validateSource('{}', 'json')).rejects.toThrow();
});
it('buffers early events and final reports until the matching start acknowledgement', async () => {
  const store = createRunStore({
    start: wailsHost.startScenario,
    cancel: wailsHost.cancelScenario,
    save: wailsHost.saveRunReport,
  });
  const pending = store.getState().start(exampleDocument);
  const socket = await socketReady();
  expect(socket.send).toHaveBeenCalledWith(expect.objectContaining({ protocolVersion: 3, format: 'json' }));
  frame(socket, { protocolVersion: 3, kind: 'event', event: runEvent('foreign') });
  frame(socket, { protocolVersion: 3, kind: 'event', event: runEvent() });
  frame(socket, { protocolVersion: 3, kind: 'finished', report: runReport() });
  frame(socket, { protocolVersion: 3, kind: 'started', runId: 'run' });
  await pending;
  expect(store.getState().status).toBe('passed');
  expect(store.getState().events).toHaveLength(1);
  expect(store.getState().report?.runId).toBe('run');
  expect(socket.close).toHaveBeenCalledTimes(1);
});
it('ignores foreign and late frames and keeps cancellation attached until completion', async () => {
  const receive = vi.fn();
  const pending = wailsHost.startScenario('{}', receive);
  const socket = await socketReady();
  frame(socket, { protocolVersion: 3, kind: 'started', runId: 'run' });
  await pending;
  frame(socket, { protocolVersion: 3, kind: 'event', event: runEvent('foreign') });
  await wailsHost.cancelScenario('run');
  expect(socket.close).not.toHaveBeenCalled();
  frame(socket, { protocolVersion: 3, kind: 'finished', report: runReport() });
  frame(socket, { protocolVersion: 3, kind: 'event', event: runEvent() });
  expect(receive).toHaveBeenCalledTimes(1);
  expect(receive.mock.calls[0][0].kind).toBe('finished');
});
it('rejects preparation errors without a synthetic success and unregisters the stream', async () => {
  const diagnostic = {
    category: 'validation',
    code: 'missingSecret',
    message: 'Missing credential',
    fieldPath: '/identities',
    nodeId: null,
    filePath: null,
    line: null,
    column: null,
  };
  const pending = wailsHost.startScenario('{}', vi.fn());
  const rejected = expect(pending).rejects.toEqual([diagnostic]);
  const socket = await socketReady();
  frame(socket, { protocolVersion: 3, kind: 'rejected', diagnostics: [diagnostic] });
  await rejected;
  expect(socket.close).toHaveBeenCalledTimes(1);
});
it('fails closed on malformed payloads, cancels its own run and releases the receiver', async () => {
  const receive = vi.fn();
  const pending = wailsHost.startScenario('{}', receive);
  const socket = await socketReady();
  frame(socket, { protocolVersion: 3, kind: 'started', runId: 'run' });
  await pending;
  frame(socket, { protocolVersion: 3, kind: 'event', event: { ...runEvent(), context: null } });
  await vi.waitFor(() => expect(fake.cancel).toHaveBeenCalledWith('run'));
  expect(receive.mock.calls[0][0].kind).toBe('failed');
  expect(socket.close).toHaveBeenCalledTimes(1);
});
it('fails a disconnected active run', async () => {
  const receive = vi.fn();
  const pending = wailsHost.startScenario('{}', receive);
  const socket = await socketReady();
  frame(socket, { protocolVersion: 3, kind: 'started', runId: 'run' });
  await pending;
  socket.onclose!();
  expect(receive).toHaveBeenCalledWith(expect.objectContaining({ kind: 'failed' }));
  expect(socket.close).toHaveBeenCalledTimes(1);
});
it('detaches the receiver on page teardown without leaking callbacks', async () => {
  const receive = vi.fn();
  const pending = wailsHost.startScenario('{}', receive);
  const socket = await socketReady();
  frame(socket, { protocolVersion: 3, kind: 'started', runId: 'run' });
  await pending;
  window.dispatchEvent(new Event('pagehide'));
  frame(socket, { protocolVersion: 3, kind: 'event', event: runEvent() });
  expect(socket.close).toHaveBeenCalledTimes(1);
  expect(receive).not.toHaveBeenCalled();
});
it('bounds pre-acknowledgement delivery', async () => {
  const pending = wailsHost.startScenario('{}', vi.fn());
  const rejected = expect(pending).rejects.toMatchObject({ code: 'executionProtocol' });
  const socket = await socketReady();
  for (let i = 0; i < 2050; i++) frame(socket, { protocolVersion: 3, kind: 'event', event: runEvent() });
  await rejected;
  expect(socket.close).toHaveBeenCalledTimes(1);
});
it('preserves dialog cancellation and returns safe file/export errors', async () => {
  fake.openDialog.mockResolvedValue('');
  fake.saveDialog.mockResolvedValue('');
  expect(await wailsHost.openScenarioFile()).toBeNull();
  expect(await wailsHost.saveScenarioJson('{}', 'example')).toBe(false);
  expect(await wailsHost.saveRunReport('run')).toBe(false);
  fake.read.mockResolvedValue({
    value: {
      path: 'example.yaml',
      format: 'yaml',
      source: 'original',
      validation: { valid: true, document: exampleDocument, diagnostics: [] },
    },
    diagnostics: [],
  });
  expect((await wailsHost.readScenarioFile('example.yaml')).source).toBe('original');
  fake.save.mockResolvedValue({ value: { valid: false, document: exampleDocument, diagnostics: [] }, diagnostics: [] });
  expect(await wailsHost.saveScenarioJson('{}', 'example', 'draft.json')).toMatchObject({
    path: 'draft.json',
    validation: { valid: false },
  });
  const diagnostic = {
    category: 'io',
    code: 'writeFile',
    message: 'Cannot save',
    fieldPath: '$',
    nodeId: null,
    filePath: null,
    line: null,
    column: null,
  };
  fake.report.mockResolvedValue({ value: null, diagnostics: [diagnostic] });
  fake.saveDialog.mockResolvedValue('report.json');
  await expect(wailsHost.saveRunReport('run')).rejects.toEqual([diagnostic]);
});
it('validates bounded synthetic probe results', async () => {
  fake.probe.mockResolvedValue([event(1)]);
  expect(await probeBridge(1)).toEqual([event(1)]);
  fake.probe.mockResolvedValue([{ ...event(1), synthetic: false }]);
  await expect(probeBridge(1)).rejects.toThrow();
  await expect(probeBridge(17)).rejects.toThrow();
});
it('subscribes before starting a stream, bounds delivery, and disposes on completion', async () => {
  fake.stream.mockImplementation(async () => {
    fake.handlers.get('interleaving:probe')!({ data: event(1) });
  });
  const receive = vi.fn();
  const stop = await streamProbe(1, receive);
  expect(receive).toHaveBeenCalledExactlyOnceWith(event(1));
  expect(fake.stop).toHaveBeenCalledTimes(1);
  stop();
  expect(fake.stop).toHaveBeenCalledTimes(1);
});
it('protects close, honors refusal, and unregisters exactly once', async () => {
  const allow = vi.fn().mockResolvedValue(false);
  const stop = await wailsHost.onCloseRequested(allow);
  expect(fake.ready).toHaveBeenCalledWith(true);
  await fake.handlers.get('interleaving:close-requested')!({ data: null });
  expect(fake.close).not.toHaveBeenCalled();
  allow.mockResolvedValue(true);
  await fake.handlers.get('interleaving:close-requested')!({ data: null });
  expect(fake.close).toHaveBeenCalledTimes(1);
  stop();
  stop();
  expect(fake.stop).toHaveBeenCalledTimes(1);
  expect(fake.ready).toHaveBeenLastCalledWith(false);
});
it('unsubscribes after a failed stream start', async () => {
  fake.stream.mockRejectedValue(new Error('bridge failed'));
  await expect(streamProbe(1, vi.fn())).rejects.toThrow('bridge failed');
  expect(fake.stop).toHaveBeenCalledTimes(1);
});
