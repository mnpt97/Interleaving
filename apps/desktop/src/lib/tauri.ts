import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { runMessageSchema, type RunMessage } from "./run-types";
import { open, save } from "@tauri-apps/plugin-dialog";
import { diagnosticSchema, openSchema, validationSchema, type ScenarioFormat } from "./scenario-types";

export const desktopAvailable = isTauri();
export async function startScenario(source: string, receive: (message: RunMessage) => void) {
  const channel = new Channel<unknown>();
  channel.onmessage = (message) => receive(runMessageSchema.parse(message));
  return invoke<{ runId: string }>("start_scenario", { source, format: "json", channel });
}
export async function cancelScenario(runId: string) { return invoke<boolean>("cancel_scenario", { runId }); }
export async function saveRunReport(runId: string) {
  const path = await save({ defaultPath: "interl-results.json", filters: [{ name: "Run report", extensions: ["json"] }] });
  if (!path) return false;
  await invoke("save_run_report", { path, runId }); return true;
}
export async function validateSource(source: string, format: ScenarioFormat) { return validationSchema.parse(await invoke("validate_scenario", { source, format })); }
export async function openScenarioFile() {
  const path = await open({ multiple: false, directory: false, filters: [{ name: "Interleaving scenarios", extensions: ["json", "yaml", "yml"] }] });
  return typeof path === "string" ? openSchema.parse(await invoke("open_scenario", { path })) : null;
}
export async function saveScenarioJson(source: string, id: string) {
  const path = await save({ defaultPath: `${id}.interl.json`, filters: [{ name: "JSON scenario", extensions: ["json"] }] });
  if (!path) return false;
  await invoke("save_scenario_json", { path, source }); return true;
}
export function errorMessage(error: unknown) {
  const diagnostic = diagnosticSchema.safeParse(error);
  return diagnostic.success ? diagnostic.data.message : "The desktop operation failed. Try again and check file permissions.";
}
