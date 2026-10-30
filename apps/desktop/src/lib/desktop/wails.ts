import { z } from 'zod';
import type { DesktopHost } from './host';
import { diagnosticSchema, openSchema, validationSchema } from '../scenario-types';
import { runMessageSchema, type RunMessage } from '../run-types';
const service = () => import('./generated/interleaving/internal/adapters/inbound/wails/service');
const runtime = () => import('@wailsio/runtime');
export const probeSchema = z
  .object({
    id: z.string().min(1).max(128),
    sequence: z.number().int().min(1).max(16),
    at: z.string(),
    synthetic: z.literal(true),
  })
  .strict();
export type ProbeEvent = z.infer<typeof probeSchema>;
/** Developer smoke utility, separate from scenario execution and never used by Run. */
export async function probeBridge(count: number) {
  if (!Number.isInteger(count) || count < 1 || count > 16) throw new Error('Probe count must be 1–16.');
  const result = await (await service()).Probe(count);
  return z.array(probeSchema).length(count).parse(result);
}
const resultSchema = z.object({ value: z.unknown(), diagnostics: z.array(diagnosticSchema) });
function unwrap(value: unknown) {
  const result = resultSchema.parse(value);
  if (result.diagnostics.length) throw result.diagnostics;
  return result.value;
}
const incompatible = () => ({
  category: 'validation',
  code: 'executionProtocol',
  message: 'Execution protocol is incompatible. Restart the desktop app.',
  fieldPath: '$',
  nodeId: null,
  filePath: null,
  line: null,
  column: null,
});

/** The official Wails stream owns ordering, bounded delivery and native teardown. */
export async function startScenario(source: string, receive: (message: RunMessage) => void) {
  const { JSONStream } = await runtime();
  return new Promise<{ runId: string; protocolVersion: number }>((resolve, reject) => {
    const socket = JSONStream('interleaving:run');
    let runId: string | null = null;
    let done = false;
    let messages = 0;
    const early: RunMessage[] = [];
    const dispose = () => {
      window.removeEventListener('pagehide', detach);
      socket.close();
    };
    const detach = () => {
      done = true;
      dispose();
    };
    window.addEventListener('pagehide', detach, { once: true });
    const fail = (reason: unknown = incompatible()) => {
      if (done) return;
      done = true;
      if (runId)
        void service()
          .then((api) => api.CancelScenario(runId!))
          .catch(() => {});
      if (runId) receive({ protocolVersion: 3, kind: 'failed', message: incompatible().message });
      else reject(reason);
      dispose();
    };
    const deliver = (message: RunMessage) => {
      if (done) return;
      if (message.kind !== 'failed') {
        const id = message.kind === 'event' ? message.event.runId : message.report.runId;
        if (id !== runId) return;
      }
      receive(message);
      if (message.kind === 'finished' || message.kind === 'failed') {
        done = true;
        dispose();
      }
    };
    socket.onopen = () => {
      if (!done) socket.send({ protocolVersion: 3, source, format: 'json' });
    };
    socket.onmessage = (event) => {
      if (done) return;
      const started = z
        .object({ protocolVersion: z.literal(3), kind: z.literal('started'), runId: z.string().min(1).max(128) })
        .strict()
        .safeParse(event.data);
      if (started.success) {
        if (runId) {
          fail();
          return;
        }
        runId = started.data.runId;
        resolve({ runId, protocolVersion: 3 });
        early.splice(0).forEach(deliver);
        return;
      }
      const rejected = z
        .object({
          protocolVersion: z.literal(3),
          kind: z.literal('rejected'),
          diagnostics: z.array(diagnosticSchema).nonempty(),
        })
        .strict()
        .safeParse(event.data);
      if (rejected.success) {
        fail(rejected.data.diagnostics);
        return;
      }
      const parsed = runMessageSchema.safeParse(event.data);
      if (!parsed.success || ++messages > 2049) {
        fail();
        return;
      }
      if (!runId) {
        early.push(parsed.data);
        return;
      }
      deliver(parsed.data);
    };
    socket.onerror = () => fail();
    socket.onclose = () => fail();
  });
}

export const wailsHost: DesktopHost = {
  kind: 'wails',
  native: true,
  available: true,
  startScenario,
  async cancelScenario(runId) {
    return (await service()).CancelScenario(runId);
  },
  async saveRunReport(runId) {
    const api = await service();
    const path = await api.SaveReportDialog();
    if (!path) return false;
    z.literal(true).parse(unwrap(await api.SaveRunReport(path, runId)));
    return true;
  },
  async validateSource(source, format) {
    return validationSchema.parse(await (await service()).ValidateSource(source, format));
  },
  async openScenarioFile() {
    const api = await service();
    const path = await api.OpenDialog();
    return path ? openSchema.parse(unwrap(await api.ReadScenario(path))) : null;
  },
  async readScenarioFile(path) {
    return openSchema.parse(unwrap(await (await service()).ReadScenario(path)));
  },
  async saveScenarioJson(source, id, destination) {
    const api = await service();
    const path = destination ?? (await api.SaveDialog(`${id}.interl.json`));
    if (!path) return false;
    return { path, validation: validationSchema.parse(unwrap(await api.SaveScenarioJSON(path, source))) };
  },
  async setTheme(theme) {
    await (await service()).ApplyTheme(theme ?? 'system');
  },
  async onCloseRequested(allow) {
    const [{ Events }, api] = await Promise.all([runtime(), service()]);
    let active = true;
    let pending = false;
    const stop = Events.On('interleaving:close-requested', async () => {
      if (!active || pending) return;
      pending = true;
      try {
        if ((await allow()) && active) await api.Close();
      } finally {
        pending = false;
      }
    });
    try {
      await api.Ready(true);
    } catch (error) {
      stop();
      throw error;
    }
    return () => {
      if (!active) return;
      active = false;
      stop();
      void api.Ready(false).catch(() => {});
    };
  },
};

/** Subscribe before invoking; delivery is bounded, validated and explicitly disposable. */
export async function streamProbe(count: number, receive: (event: ProbeEvent) => void) {
  if (!Number.isInteger(count) || count < 1 || count > 16) throw new Error('Probe count must be 1–16.');
  const [{ Events }, api] = await Promise.all([runtime(), service()]);
  let active = true;
  let sequence = 0;
  let identity: string | null = null;
  const unsubscribe = Events.On('interleaving:probe', (message) => {
    if (!active) return;
    const parsed = probeSchema.safeParse(message.data);
    if (!parsed.success) {
      active = false;
      unsubscribe();
      throw new Error('Malformed probe event.');
    }
    const event = parsed.data;
    if (event.sequence !== sequence + 1 || event.sequence > count || (identity !== null && event.id !== identity)) {
      active = false;
      unsubscribe();
      throw new Error('Invalid probe sequence.');
    }
    sequence = event.sequence;
    identity = event.id;
    receive(event);
    if (sequence === count) {
      active = false;
      unsubscribe();
    }
  });
  try {
    await api.StreamProbe(count);
  } catch (error) {
    active = false;
    unsubscribe();
    throw error;
  }
  return () => {
    if (!active) return;
    active = false;
    unsubscribe();
  };
}
