import { create } from "zustand";
import type { ScenarioDocument } from "../../lib/scenario-types";
import { diagnosticSchema } from "../../lib/scenario-types";
import type { RunEvent, RunMessage, RunReport } from "../../lib/run-types";
import { cancelScenario, saveRunReport, startScenario } from "../../lib/tauri";
interface Bridge { start(source: string, receive: (message: RunMessage) => void): Promise<{ runId: string }>; cancel(runId: string): Promise<boolean>; save(runId: string): Promise<boolean> }
export interface RunState {
  status: "idle" | "starting" | "running" | "cancelling" | RunReport["status"]; runId: string | null; scenarioId: string | null;
  events: RunEvent[]; report: RunReport | null; error: string | null; notice: string | null; selected: number | null;
  start(document: ScenarioDocument): Promise<void>; cancel(): Promise<void>; select(sequence: number): void; save(): Promise<void>;
}
export const activeRun = (status: RunState["status"]) => ["starting", "running", "cancelling"].includes(status);
export function createRunStore(bridge: Bridge) {
  let generation = 0;
  return create<RunState>((set, get) => ({
    status: "idle", runId: null, scenarioId: null, events: [], report: null, error: null, notice: null, selected: null,
    select: (selected) => set({ selected }),
    start: async (document) => {
      if (activeRun(get().status)) return;
      const current = ++generation;
      // Capture the source before awaiting IPC. Editor state remains independent.
      const source = JSON.stringify(document);
      set({ status: "starting", runId: null, scenarioId: document.id, events: [], report: null, error: null, notice: null, selected: null });
      try {
        const started = await bridge.start(source, (message) => {
          if (current !== generation) return;
          if (message.kind === "failed") { set({ status: "error", error: message.message }); return; }
          const id = message.kind === "event" ? message.event.runId : message.report.runId;
          if (get().runId && get().runId !== id) return;
          if (message.kind === "finished") {
            set({ runId: id, status: message.report.status, report: message.report, events: message.report.events, error: message.report.error && message.report.status !== "cancelled" ? `${message.report.error.kind}: ${message.report.error.message}` : null });
          } else if (!get().report) {
            const event = message.event;
            if (get().events.some((previous) => previous.sequence === event.sequence)) return;
            set({ runId: id, status: get().status === "cancelling" ? "cancelling" : "running", events: [...get().events, event].slice(-2048) });
          }
        });
        if (current === generation && !get().report && activeRun(get().status)) set({ runId: started.runId, status: get().status === "cancelling" ? "cancelling" : "running" });
      } catch (error) {
        const diagnostics = diagnosticSchema.array().safeParse(error);
        if (current === generation && !get().report) set({ status: "error", error: diagnostics.success ? diagnostics.data.map((d) => d.message).join(" ") : "Run could not start. M2 requires one group with one user, valid paths, and timeouts up to 300000 ms. Validate the scenario and try again." });
      }
    },
    cancel: async () => {
      const { runId, status } = get(); if (!runId || !activeRun(status) || status === "cancelling") return;
      const current = generation; set({ status: "cancelling", error: null });
      try { const accepted = await bridge.cancel(runId); if (current === generation && !get().report && !accepted) set({ status: "running", error: "This run is no longer active. Waiting for its final report." }); }
      catch { if (current === generation && !get().report) set({ status: "running", error: "Cancellation could not be sent. Try again." }); }
    },
    save: async () => {
      const report = get().report; if (!report) return; const current = generation;
      try { const saved = await bridge.save(report.runId); if (current === generation) set({ notice: saved ? "Redacted JSON report saved." : null }); }
      catch { if (current === generation) set({ error: "Report could not be saved. Check file permissions and try again." }); }
    },
  }));
}
export const useRunStore = createRunStore({ start: startScenario, cancel: cancelScenario, save: saveRunReport });
