import { create } from "zustand";
import exampleSource from "../../../../../scenarios/examples/hello-api.json?raw";
import { documentSchema, type Diagnostic, type OpenScenarioResult, type ScenarioDocument, type ScenarioFormat, type ValidationResult } from "../../lib/scenario-types";
import { desktopAvailable, errorMessage, openScenarioFile, saveScenarioJson, validateSource } from "../../lib/tauri";
import { moveNode } from "./adapters";

export const exampleDocument = documentSchema.parse(JSON.parse(exampleSource));
type Status = "preview" | "unchecked" | "valid" | "invalid";
interface Bridge { validate(source: string, format: ScenarioFormat): Promise<ValidationResult>; open(): Promise<OpenScenarioResult | null>; save(source: string, id: string): Promise<boolean> }
export interface ScenarioState {
  document: ScenarioDocument | null; diagnostics: Diagnostic[]; path: string | null; format: ScenarioFormat; originalSource: string;
  selectedId: string | null; status: Status; busy: boolean; validating: boolean; layoutDirty: boolean; error: string | null; notice: string | null; revision: number; fileRevision: number;
  select(id: string | null): void; move(id: string, position: { x: number; y: number }): void;
  loadExample(): Promise<void>; open(): Promise<void>; validate(): Promise<void>; save(): Promise<void>;
}

export function createScenarioStore(bridge: Bridge, native = true) {
  return create<ScenarioState>((set, get) => {
    function begin(validating = false) { const revision = get().revision + 1; set({ revision, busy: true, validating, error: null, notice: null }); return revision; }
    function apply(result: ValidationResult, revision: number) { if (get().revision === revision) set({ document: result.document, diagnostics: result.diagnostics, status: result.valid ? "valid" : "invalid", busy: false, validating: false }); }
    function failed(error: unknown, revision: number) { if (get().revision === revision) set({ busy: false, validating: false, error: errorMessage(error) }); }
    return {
      document: exampleDocument, diagnostics: [], path: null, format: "json", originalSource: exampleSource, selectedId: "health", status: native ? "unchecked" : "preview", busy: false, validating: false, layoutDirty: false, error: null, notice: null, revision: 0, fileRevision: 0,
      select: (selectedId) => set({ selectedId }),
      move: (id, position) => { const document = get().document; if (!document) return; const moved = moveNode(document, id, position); if (moved === document) return; set({ document: moved, layoutDirty: true, revision: get().revision + 1, busy: false, validating: false, notice: null }); },
      loadExample: async () => { if (!native) return; const revision = begin(true); try { const result = await bridge.validate(exampleSource, "json"); if (get().revision !== revision) return; set({ path: null, format: "json", originalSource: exampleSource, layoutDirty: false, fileRevision: get().fileRevision + 1, selectedId: "health" }); apply(result, revision); } catch (error) { failed(error, revision); } },
      open: async () => { const revision = begin(true); try { const result = await bridge.open(); if (get().revision !== revision) return; if (!result) { set({ busy: false, validating: false }); return; } set({ path: result.path, format: result.format, originalSource: result.source, layoutDirty: false, selectedId: result.validation.document?.workflow.nodes[0]?.id ?? null, fileRevision: get().fileRevision + 1 }); apply(result.validation, revision); } catch (error) { failed(error, revision); } },
      validate: async () => { const current = get(); const source = current.document ? JSON.stringify(current.document) : current.originalSource; const format = current.document ? "json" : current.format; const revision = begin(true); try { apply(await bridge.validate(source, format), revision); } catch (error) { failed(error, revision); } },
      save: async () => { const document = get().document; if (!document) return; const revision = begin(); try { const saved = await bridge.save(JSON.stringify(document), document.id); if (get().revision === revision) set({ busy: false, notice: saved ? "JSON copy saved. The original file is unchanged." : null }); } catch (error) { failed(error, revision); } },
    };
  });
}
export const useScenarioStore = createScenarioStore({ validate: validateSource, open: openScenarioFile, save: saveScenarioJson }, desktopAvailable);
