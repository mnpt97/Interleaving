import { projectRun } from './projection';
import { create } from 'zustand';
import type { ScenarioDocument } from '../../lib/scenario-types';
import type { RunEvent, RunMessage, RunReport } from '../../lib/run-types';
import { cancelScenario, errorMessage, saveRunReport, startScenario } from '../../lib/desktop';
interface Bridge {
  start(source: string, receive: (message: RunMessage) => void): Promise<{ runId: string }>;
  cancel(runId: string): Promise<boolean>;
  save(runId: string): Promise<boolean>;
}
export interface RunState {
  status: 'idle' | 'starting' | 'running' | 'cancelling' | RunReport['status'];
  runId: string | null;
  scenarioId: string | null;
  capturedDocument: ScenarioDocument | null;
  events: RunEvent[];
  report: RunReport | null;
  error: string | null;
  notice: string | null;
  selected: number | null;
  showRun: boolean;
  canvasView: 'lifecycle' | 'invocation';
  openInvocation(key: string): void;
  showLifecycle(): void;
  snapshotDismissed: boolean;
  selectedInvocation: string | null;
  selectedWorkflow: string | null;
  selectedNode: string | null;
  selectedExecution: string | null;
  expanded: Record<string, boolean>;
  streamIncomplete: boolean;
  showSnapshot(show: boolean): void;
  inspectInvocation(key: string): void;
  inspectWorkflow(key: string): void;
  inspectNode(key: string | null, invocation?: string): void;
  toggleExecution(key: string): void;
  start(document: ScenarioDocument): Promise<void>;
  cancel(): Promise<void>;
  select(sequence: number): void;
  save(): Promise<void>;
}
export const activeRun = (status: RunState['status']) => ['starting', 'running', 'cancelling'].includes(status);
export function createRunStore(bridge: Bridge) {
  let generation = 0;
  return create<RunState>((set, get) => ({
    status: 'idle',
    runId: null,
    scenarioId: null,
    capturedDocument: null,
    events: [],
    report: null,
    error: null,
    notice: null,
    selected: null,
    canvasView: 'lifecycle',
    openInvocation: (key) => {
      get().inspectInvocation(key);
      set({ canvasView: 'invocation' });
    },
    showLifecycle: () => {
      const projection = projectRun(get().events, get().report);
      const current = projection.invocations.find((value) => value.key === get().selectedInvocation);
      const root = projection.invocations.find(
        (value) =>
          !value.parentId &&
          value.phase === current?.phase &&
          value.groupKey === current?.groupKey &&
          value.userId === current?.userId &&
          value.iteration === current?.iteration
      );
      set({
        canvasView: 'lifecycle',
        selectedInvocation: root?.key ?? null,
        selectedWorkflow: root?.workflowKey ?? null,
        selectedNode: null,
        selectedExecution: null,
        selected: null,
      });
    },
    showRun: false,
    snapshotDismissed: false,
    selectedInvocation: null,
    selectedWorkflow: null,
    selectedNode: null,
    selectedExecution: null,
    expanded: {},
    streamIncomplete: false,
    showSnapshot: (showRun) => set({ showRun, snapshotDismissed: !showRun }),
    inspectInvocation: (selectedInvocation) => {
      const invocation = projectRun(get().events, get().report).invocations.find(
        (value) => value.key === selectedInvocation
      );
      if (invocation)
        set({
          canvasView: invocation.parentId ? 'invocation' : 'lifecycle',
          selectedInvocation,
          selectedWorkflow: invocation.workflowKey,
          selectedNode: null,
          selectedExecution: null,
          selected: null,
          showRun: true,
          snapshotDismissed: false,
        });
    },
    inspectWorkflow: (selectedWorkflow) => {
      const projection = projectRun(get().events, get().report);
      const invocation = [...projection.invocations].reverse().find((value) => value.workflowKey === selectedWorkflow);
      set({
        canvasView: 'invocation',
        selectedWorkflow,
        selectedInvocation: invocation?.key ?? null,
        selectedNode: null,
        selectedExecution: null,
        selected: null,
      });
    },
    inspectNode: (selectedNode, scope) => {
      const invocation = scope
        ? projectRun(get().events, get().report).invocations.find((value) => value.key === scope)
        : undefined;
      set({
        selectedNode,
        selectedExecution: null,
        selected: null,
        ...(invocation ? { selectedInvocation: invocation.key, selectedWorkflow: invocation.workflowKey } : {}),
      });
    },
    toggleExecution: (key) => set({ expanded: { ...get().expanded, [key]: !get().expanded[key] } }),
    select: (selected) => {
      const projection = projectRun(get().events, get().report);
      for (const invocation of projection.invocations) {
        const node = invocation.nodes.find((node) => node.events.some((event) => event.sequence === selected));
        if (node) {
          set({
            canvasView: invocation.parentId ? 'invocation' : 'lifecycle',
            selected,
            selectedInvocation: invocation.key,
            selectedWorkflow: invocation.workflowKey,
            selectedNode: node.nodeKey,
            selectedExecution: node.key,
          });
          return;
        }
      }
      set({ selected });
    },
    start: async (document) => {
      if (activeRun(get().status)) return;
      const current = ++generation;
      // Capture the source before awaiting IPC. Editor state remains independent.
      const source = JSON.stringify(document);
      set({
        status: 'starting',
        canvasView: 'lifecycle',
        runId: null,
        scenarioId: document.id,
        capturedDocument: JSON.parse(source) as ScenarioDocument,
        events: [],
        report: null,
        error: null,
        notice: null,
        selected: null,
        showRun: false,
        snapshotDismissed: false,
        selectedInvocation: null,
        selectedWorkflow: null,
        selectedNode: null,
        selectedExecution: null,
        expanded: {},
        streamIncomplete: false,
      });
      try {
        let acknowledged: string | null = null;
        const buffered: RunMessage[] = [];
        let stopped = false;
        const receive = (message: RunMessage) => {
          if (current !== generation || stopped) return;
          if (!acknowledged) {
            if (buffered.length >= 2049) {
              buffered.shift();
              set({ streamIncomplete: true });
            }
            buffered.push(message);
            return;
          }
          if (current !== generation) return;
          if (get().report) return;
          if (message.kind === 'failed') {
            stopped = true;
            set({ status: 'error', error: message.message });
            return;
          }
          const id = message.kind === 'event' ? message.event.runId : message.report.runId;
          if (acknowledged !== id) return;
          if (message.kind === 'finished') {
            set({
              runId: id,
              status: message.report.status,
              report: message.report,
              events: [...new Map(message.report.events.map((event) => [event.sequence, event])).values()].sort(
                (a, b) => a.sequence - b.sequence
              ),
              showRun: !get().snapshotDismissed,
              selected: message.report.events.some((event) => event.sequence === get().selected)
                ? get().selected
                : null,
              error:
                message.report.error && message.report.status !== 'cancelled'
                  ? `${message.report.error.kind}: ${message.report.error.message}`
                  : null,
            });
          } else if (!get().report) {
            const event = message.event;
            if (get().events.some((previous) => previous.sequence === event.sequence)) return;
            set({
              runId: id,
              status: get().status === 'cancelling' ? 'cancelling' : 'running',
              events: [...get().events, event].sort((a, b) => a.sequence - b.sequence).slice(-2048),
              streamIncomplete: get().streamIncomplete || get().events.length >= 2048,
              showRun: !get().snapshotDismissed && (get().showRun || event.type === 'run.started'),
            });
          }
        };
        const started = await bridge.start(source, receive);
        acknowledged = started.runId;
        if (current !== generation) return;
        set({ runId: started.runId });
        buffered.forEach(receive);
        if (current === generation && !get().report && activeRun(get().status))
          set({ runId: started.runId, status: get().status === 'cancelling' ? 'cancelling' : 'running' });
      } catch (error) {
        if (current === generation && !get().report)
          set({
            status: 'error',
            error: errorMessage(
              error,
              'Run could not start. Validate the scenario and check its required environment references.'
            ),
          });
      }
    },
    cancel: async () => {
      const { runId, status } = get();
      if (!runId || !activeRun(status) || status === 'cancelling') return;
      const current = generation;
      set({ status: 'cancelling', error: null });
      try {
        const accepted = await bridge.cancel(runId);
        if (current === generation && !get().report && !accepted)
          set({ status: 'running', error: 'This run is no longer active. Waiting for its final report.' });
      } catch {
        if (current === generation && !get().report)
          set({ status: 'running', error: 'Cancellation could not be sent. Try again.' });
      }
    },
    save: async () => {
      const report = get().report;
      if (!report) return;
      const current = generation;
      try {
        const saved = await bridge.save(report.runId);
        if (current === generation) set({ notice: saved ? 'Redacted JSON report saved.' : null });
      } catch {
        if (current === generation) set({ error: 'Report could not be saved. Check file permissions and try again.' });
      }
    },
  }));
}
export const useRunStore = createRunStore({ start: startScenario, cancel: cancelScenario, save: saveRunReport });
