import { phaseWorkflow, phases, ownsPhase, canMovePhaseFields, type Phase } from './lifecycle';
import { create } from 'zustand';
import exampleSource from '../../../../../scenarios/examples/hello-api.json?raw';
import {
  documentSchema,
  iterationWorkflow,
  type Diagnostic,
  type OpenScenarioResult,
  type ScenarioDocument,
  type ScenarioFormat,
  type ValidationResult,
} from '../../lib/scenario-types';
import {
  desktopAvailable,
  errorMessage,
  openScenarioFile,
  readScenarioFile,
  saveScenarioJson,
  validateSource,
} from '../../lib/tauri';
import { activeRun, useRunStore } from '../runs/store';
import { moveNode } from './adapters';
import { executeCommand, newDraft, type EditorCommand } from './draft';

export const exampleDocument = documentSchema.parse(JSON.parse(exampleSource));
export interface ScenarioEntry {
  key: string;
  name: string;
  path: string | null;
  source: string;
  format: ScenarioFormat;
}
const bundled = import.meta.glob('../../../../../scenarios/examples/*.json', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;
const entries: ScenarioEntry[] = Object.entries(bundled).flatMap(([key, source]) => {
  if (key.endsWith('/hello-api.interl.json')) return [];
  try {
    const parsed = documentSchema.safeParse(JSON.parse(source));
    return parsed.success ? [{ key, name: parsed.data.name, source, path: null, format: 'json' as const }] : [];
  } catch {
    return [];
  }
});
export interface SaveResult {
  path: string;
  validation: ValidationResult;
}
type Status = 'preview' | 'unchecked' | 'valid' | 'invalid';
interface Bridge {
  validate(source: string, format: ScenarioFormat): Promise<ValidationResult>;
  open(): Promise<OpenScenarioResult | null>;
  read?(path: string): Promise<OpenScenarioResult>;
  save(source: string, id: string, destination?: string | null): Promise<boolean | ValidationResult | SaveResult>;
}
export interface ScenarioState {
  mode: 'view' | 'edit';
  canvasView: 'lifecycle' | 'workflow';
  selectedPhase: Phase;
  lifecycleCaller: boolean;
  selectPhase(phase: Phase, nodeId?: string | null): void;
  showLifecycle(): void;
  entries: ScenarioEntry[];
  entryKey: string;
  savePath: string | null;
  pendingChange: boolean;
  saving: boolean;
  resolveChanges(choice: 'save' | 'discard' | 'cancel'): Promise<void>;
  setMode(mode: 'view' | 'edit'): Promise<boolean>;
  switchScenario(key: string): Promise<void>;
  requestClose(): Promise<boolean>;

  dirty: boolean;
  selectedWorkflowId: string | null;
  workflowTrail: { workflowId: string; nodeId: string }[];
  enterWorkflow(nodeId: string): void;
  leaveWorkflow(): void;
  past: ScenarioDocument[];
  future: ScenarioDocument[];
  savedDocument: string;
  formErrors: Record<string, string>;
  setFormError(field: string, error: string | null): void;
  command(command: EditorCommand, coalesceKey?: string): void;
  newScenario(): Promise<void>;
  selectWorkflow(id: string): void;
  endHistoryGroup(): void;
  undo(): void;
  redo(): void;
  document: ScenarioDocument | null;
  diagnostics: Diagnostic[];
  path: string | null;
  format: ScenarioFormat;
  originalSource: string;
  selectedId: string | null;
  inspectorOpen: boolean;
  inspectorTab: 'node' | 'workflow' | 'scenario';
  focusedFieldPath: string | null;
  setInspectorTab(tab: 'node' | 'workflow' | 'scenario'): void;
  inspectDiagnostic(diagnostic: Diagnostic): void;
  toggleInspector(): void;
  status: Status;
  busy: boolean;
  validating: boolean;
  layoutDirty: boolean;
  error: string | null;
  notice: string | null;
  revision: number;
  fileRevision: number;
  select(id: string | null): void;
  move(id: string, position: { x: number; y: number }): void;
  loadExample(): Promise<void>;
  open(): Promise<void>;
  validate(): Promise<void>;
  save(): Promise<boolean>;
}

export function createScenarioStore(bridge: Bridge, native = true, confirmDiscard?: () => boolean) {
  return create<ScenarioState>((set, get) => {
    let historyKey: string | undefined;
    let resolvePending: ((value: boolean) => void) | undefined;
    let baselineStatus: Status = native ? 'unchecked' : 'preview';
    let baselineDiagnostics: Diagnostic[] = [];
    let newOrigin: Pick<
      ScenarioState,
      | 'document'
      | 'path'
      | 'savePath'
      | 'format'
      | 'originalSource'
      | 'entryKey'
      | 'selectedWorkflowId'
      | 'savedDocument'
      | 'status'
      | 'diagnostics'
    > | null = null;
    function discard() {
      set({ workflowTrail: [] });
      if (newOrigin) {
        set({
          ...newOrigin,
          dirty: false,
          formErrors: {},
          past: [],
          future: [],
          revision: get().revision + 1,
          fileRevision: get().fileRevision + 1,
        });
        newOrigin = null;
        return;
      }
      const document = get().savedDocument ? documentSchema.parse(JSON.parse(get().savedDocument)) : null;
      set({
        document,
        dirty: false,
        formErrors: {},
        past: [],
        future: [],
        status: baselineStatus,
        diagnostics: baselineDiagnostics,
        revision: get().revision + 1,
        fileRevision: get().fileRevision + 1,
      });
    }
    async function canReplace() {
      if ((get().busy && !get().validating) || get().saving || get().pendingChange) return false;
      if (!get().dirty && !Object.keys(get().formErrors).length) return true;
      if (confirmDiscard) {
        if (!confirmDiscard()) return false;
        discard();
        return true;
      }
      set({ pendingChange: true });
      return new Promise<boolean>((resolve) => {
        resolvePending = resolve;
      });
    }
    function register(entry: ScenarioEntry) {
      set({ entries: [...get().entries.filter((item) => item.key !== entry.key), entry], entryKey: entry.key });
    }

    function replace(document: ScenarioDocument, key?: string) {
      const current = get();
      if (current.mode !== 'edit' || current.pendingChange) return;
      if (JSON.stringify(document) === JSON.stringify(current.document)) return;
      const executableSource = (value: ScenarioDocument | null) =>
        value &&
        JSON.stringify({
          ...value,
          workflows: value.workflows.map(({ layout: _layout, ...workflow }) => workflow),
        });
      const layoutOnly = executableSource(document) === executableSource(current.document);
      set({
        document,
        past:
          current.document && (!key || key !== historyKey)
            ? [...current.past, current.document].slice(-100)
            : current.past,
        future: [],
        dirty: JSON.stringify(document) !== current.savedDocument,
        status: layoutOnly ? current.status : 'unchecked',
        diagnostics: layoutOnly ? current.diagnostics : [],
        revision: current.revision + 1,
        busy: false,
        validating: false,
        error: null,
        notice: null,
      });
      if (!document.workflows.some((workflow) => workflow.id === get().selectedWorkflowId))
        set({ selectedWorkflowId: iterationWorkflow(document)?.id ?? null, selectedId: null, workflowTrail: [] });
      if (get().canvasView === 'lifecycle') {
        const flow = phaseWorkflow(document, get().selectedPhase);
        set({
          selectedWorkflowId: flow?.id ?? null,
          ...(get().selectedId && !flow?.nodes.some((node) => node.id === get().selectedId)
            ? { selectedId: null }
            : {}),
        });
      }
      historyKey = key;
    }
    function restore(direction: 'past' | 'future') {
      const current = get();
      if (current.mode !== 'edit' || current.pendingChange) return;
      const stack = current[direction];
      const document = stack[stack.length - 1];
      if (!document || !current.document) return;
      if (
        Object.keys(current.formErrors).some(
          (key) => key.startsWith('configuration:') || key.startsWith('fragment:') || key.startsWith('authentication:')
        )
      ) {
        set({ error: 'Apply or discard configuration fields before undoing or redoing document changes.' });
        return;
      }
      replace(document);
      const oldWorkflow = current.document.workflows.find((workflow) => workflow.id === current.selectedWorkflowId);
      const restoredWorkflow = document.workflows.find((workflow) => workflow.id === current.selectedWorkflowId);
      const index = oldWorkflow?.nodes.findIndex((node) => node.id === current.selectedId) ?? -1;
      const restoredNode = restoredWorkflow?.nodes[index];
      if (
        index >= 0 &&
        restoredNode &&
        oldWorkflow?.nodes.length === restoredWorkflow?.nodes.length &&
        !restoredWorkflow.nodes.some((node) => node.id === current.selectedId) &&
        !oldWorkflow.nodes.some((node) => node.id === restoredNode.id)
      )
        set({ selectedId: restoredNode.id, focusedFieldPath: null });
      set(
        direction === 'past'
          ? { past: stack.slice(0, -1), future: [...current.future, current.document] }
          : { future: stack.slice(0, -1), past: [...current.past, current.document] }
      );
      historyKey = undefined;
    }
    function reset(document: ScenarioDocument | null) {
      useRunStore.getState().showSnapshot(false);
      baselineStatus = 'unchecked';
      baselineDiagnostics = [];
      historyKey = undefined;
      set({
        dirty: false,
        past: [],
        future: [],
        formErrors: {},
        savedDocument: JSON.stringify(document),
        selectedWorkflowId: document ? (iterationWorkflow(document)?.id ?? null) : null,
        workflowTrail: [],
        canvasView: 'lifecycle',
        selectedPhase: 'iteration',
        lifecycleCaller: false,
        inspectorTab: 'node',
        focusedFieldPath: null,
      });
    }
    function begin(validating = false) {
      historyKey = undefined;
      const revision = get().revision + 1;
      set({ revision, busy: true, validating, error: null, notice: null });
      return revision;
    }
    function apply(result: ValidationResult, revision: number, preserveDocument = false) {
      if (get().revision === revision)
        set({
          document: preserveDocument ? (get().document ?? result.document) : (result.document ?? get().document),
          diagnostics: result.diagnostics,
          status: result.valid ? 'valid' : 'invalid',
          busy: false,
          validating: false,
        });
    }
    function failed(error: unknown, revision: number) {
      if (get().revision === revision) set({ busy: false, validating: false, error: errorMessage(error) });
    }
    return {
      mode: 'view',
      entries,
      entryKey: Object.keys(bundled).find((key) => key.endsWith('/hello-api.json')) ?? '',
      savePath: null,
      pendingChange: false,
      saving: false,
      resolveChanges: async (choice) => {
        if (!resolvePending) return;
        if (choice === 'save' && !(await get().save())) return;
        if (choice === 'discard') discard();
        const resolve = resolvePending;
        resolvePending = undefined;
        set({ pendingChange: false });
        resolve(choice !== 'cancel');
      },
      requestClose: canReplace,
      setMode: async (mode) => {
        if (mode === get().mode) return true;
        if (!(await canReplace())) return false;
        historyKey = undefined;
        if (mode === 'edit') {
          baselineStatus = get().status;
          baselineDiagnostics = get().diagnostics;
        }
        set({ mode, past: [], future: [], formErrors: {} });
        return true;
      },
      switchScenario: async (key) => {
        if (activeRun(useRunStore.getState().status)) return;
        const entry = get().entries.find((item) => item.key === key);
        if (!entry || get().pendingChange || get().saving) return;
        let revision = begin(true);
        try {
          const opened =
            entry.path && bridge.read
              ? await bridge.read(entry.path)
              : {
                  path: entry.path,
                  source: entry.source,
                  format: entry.format,
                  validation: native
                    ? await bridge.validate(entry.source, entry.format)
                    : { valid: false, document: documentSchema.parse(JSON.parse(entry.source)), diagnostics: [] },
                };
          if (get().revision !== revision) return;
          if (!opened.validation.document) {
            set({
              diagnostics: opened.validation.diagnostics,
              busy: false,
              validating: false,
              error: 'Cannot open this scenario. Review the file diagnostics.',
            });
            return;
          }
          set({ busy: false, validating: false });
          if (!(await canReplace())) return;
          revision = begin(true);
          reset(opened.validation.document);
          register({
            ...entry,
            name: opened.validation.document.name,
            source: opened.source,
            path: opened.path,
            format: opened.format,
          });
          set({
            document: opened.validation.document,
            path: opened.path,
            format: opened.format,
            originalSource: opened.source,
            savePath: opened.path && opened.format === 'json' ? opened.path : null,
            entryKey: key,
            mode: 'view',
            selectedId: null,
            fileRevision: get().fileRevision + 1,
          });
          apply(opened.validation, revision);
          if (!native) set({ status: 'preview' });
        } catch (error) {
          failed(error, revision);
        }
      },
      dirty: false,
      canvasView: 'lifecycle',
      selectedPhase: 'iteration',
      lifecycleCaller: false,
      showLifecycle: () => get().selectPhase(get().selectedPhase),
      selectPhase: (phase, nodeId = null) => {
        if (Object.keys(get().formErrors).length && (phase !== get().selectedPhase || nodeId !== get().selectedId)) {
          set({ error: 'Apply or discard the current fields before changing phases.' });
          return;
        }
        const document = get().document;
        const workflow = document && phaseWorkflow(document, phase);
        set({
          canvasView: 'lifecycle',
          selectedPhase: phase,
          selectedWorkflowId: workflow?.id ?? null,
          selectedId: nodeId,
          workflowTrail: [],
          inspectorOpen: true,
          inspectorTab: nodeId ? 'node' : 'workflow',
          focusedFieldPath: null,
        });
      },
      selectedWorkflowId: iterationWorkflow(exampleDocument)?.id ?? null,
      workflowTrail: [],
      past: [],
      future: [],
      savedDocument: JSON.stringify(exampleDocument),
      formErrors: {},
      setFormError: (field, error) => {
        const formErrors = { ...get().formErrors };
        if (error) formErrors[field] = error;
        else delete formErrors[field];
        set({ formErrors });
      },
      command: (command, key) => {
        if (get().mode !== 'edit' || get().pendingChange) return;
        try {
          if (
            command.type.startsWith('phase.') &&
            Object.keys(get().formErrors).length &&
            !(
              command.type === 'phase.move' &&
              get().document &&
              canMovePhaseFields(get().document!, get().formErrors, command.from, command.nodeIds)
            )
          )
            throw new Error('Apply or discard the current fields before changing phases.');
          if (get().document) {
            const before = get().document!;
            const next = executeCommand(before, command);
            replace(next, key);
            if (
              command.type === 'phase.create' ||
              command.type === 'phase.detach' ||
              command.type === 'phase.node.add'
            ) {
              const flow = phaseWorkflow(next, command.phase);
              const previous = phaseWorkflow(before, command.phase);
              const node =
                command.type === 'phase.node.add'
                  ? flow?.nodes.find(
                      (node) =>
                        !previous?.nodes.some((old) => old.id === node.id) &&
                        node.type !== 'start' &&
                        node.type !== 'end'
                    )
                  : undefined;
              set({
                canvasView: 'lifecycle',
                selectedPhase: command.phase,
                selectedWorkflowId: flow?.id ?? null,
                selectedId: node?.id ?? null,
                inspectorOpen: true,
                inspectorTab: node ? 'node' : 'workflow',
                focusedFieldPath: null,
              });
            }
            if (command.type === 'phase.move') {
              const flow = phaseWorkflow(next, command.to);
              set({
                canvasView: 'lifecycle',
                selectedPhase: command.to,
                selectedWorkflowId: flow?.id ?? null,
                selectedId: command.nodeIds[0] ?? null,
                inspectorOpen: true,
                inspectorTab: 'node',
              });
            }
            if (command.type === 'node.rename') set({ selectedId: command.nextId, focusedFieldPath: null });
            if (command.type === 'node.change')
              set({
                selectedId: command.nodeId,
                selectedWorkflowId: command.workflowId,
                inspectorOpen: true,
                inspectorTab: 'node',
                focusedFieldPath: null,
              });
            if (command.type === 'graph.delete' || command.type === 'node.delete') {
              const workflow = next.workflows.find((workflow) => workflow.id === command.workflowId);
              const selectedId = get().selectedId;
              if (selectedId && !workflow?.nodes.some((node) => node.id === selectedId)) set({ selectedId: null });
              set({
                notice:
                  'Deleted from the draft. Attached connections were removed with deleted nodes. Undo restores the edit; reconnect any remaining gaps before running.',
              });
            }
            if (
              command.type === 'http.insert' ||
              command.type === 'authentication.insert' ||
              command.type === 'node.insert' ||
              command.type === 'node.add' ||
              command.type === 'boundary.add'
            ) {
              const previous = before.workflows.find((w) => w.id === command.workflowId)!;
              const node = next.workflows
                .find((w) => w.id === command.workflowId)
                ?.nodes.find((n) => !previous.nodes.some((old) => old.id === n.id));
              if (node) set({ selectedId: node.id, inspectorOpen: true, inspectorTab: 'node', focusedFieldPath: null });
            }
          }
        } catch (error) {
          set({ error: error instanceof Error ? error.message : 'Invalid edit.' });
        }
      },
      newScenario: async () => {
        if (activeRun(useRunStore.getState().status) || !(await canReplace())) return;
        const {
          document: oldDocument,
          path,
          savePath,
          format,
          originalSource,
          entryKey,
          selectedWorkflowId,
          savedDocument,
          status,
          diagnostics,
        } = get();
        const document = newDraft();
        document.workflows[0].edges = [];
        reset(document);
        newOrigin = {
          document: oldDocument,
          path,
          savePath,
          format,
          originalSource,
          entryKey,
          selectedWorkflowId,
          savedDocument,
          status,
          diagnostics,
        };
        set({
          document,
          dirty: true,
          mode: 'edit',
          entryKey: '',
          savePath: null,
          path: null,
          format: 'json',
          originalSource: '',
          selectedId: 'start',
          status: 'unchecked',
          revision: get().revision + 1,
          fileRevision: get().fileRevision + 1,
          busy: false,
          validating: false,
          diagnostics: [],
        });
      },
      selectWorkflow: (id) => {
        if (Object.keys(get().formErrors).length) {
          set({ error: 'Finish the current fields or discard changes before switching workflows.' });
          return;
        }
        if (get().document?.workflows.some((w) => w.id === id)) {
          historyKey = undefined;
          set({
            canvasView: 'workflow',
            lifecycleCaller: false,
            selectedWorkflowId: id,
            selectedId: null,
            workflowTrail: [],
            fileRevision: get().fileRevision + 1,
          });
        }
      },
      enterWorkflow: (nodeId) => {
        if (Object.keys(get().formErrors).length) {
          set({ error: 'Finish the current fields before opening a workflow.' });
          return;
        }
        const document = get().document;
        const workflow = document?.workflows.find((workflow) => workflow.id === get().selectedWorkflowId);
        const node = workflow?.nodes.find((node) => node.id === nodeId);
        if (node?.type !== 'call' || !document?.workflows.some((workflow) => workflow.id === node.config.workflowId)) {
          set({ error: 'Called workflow is unavailable.' });
          return;
        }
        if (get().workflowTrail.length >= 8) {
          set({ error: 'Workflow navigation depth is limited to eight calls.' });
          return;
        }
        historyKey = undefined;
        set({
          canvasView: 'workflow',
          lifecycleCaller: get().workflowTrail.length ? get().lifecycleCaller : get().canvasView === 'lifecycle',
          selectedWorkflowId: node.config.workflowId,
          selectedId: null,
          workflowTrail: [...get().workflowTrail, { workflowId: workflow!.id, nodeId }],
          fileRevision: get().fileRevision + 1,
        });
      },
      leaveWorkflow: () => {
        if (Object.keys(get().formErrors).length) {
          set({ error: 'Finish the current fields before returning to the caller.' });
          return;
        }
        const parent = get().workflowTrail[get().workflowTrail.length - 1];
        if (!parent) return;
        historyKey = undefined;
        set({
          canvasView: get().workflowTrail.length === 1 && get().lifecycleCaller ? 'lifecycle' : 'workflow',
          selectedWorkflowId: parent.workflowId,
          selectedId: parent.nodeId,
          workflowTrail: get().workflowTrail.slice(0, -1),
          fileRevision: get().fileRevision + 1,
        });
      },
      endHistoryGroup: () => {
        historyKey = undefined;
      },
      undo: () => restore('past'),
      redo: () => restore('future'),
      document: exampleDocument,
      diagnostics: [],
      path: null,
      format: 'json',
      originalSource: exampleSource,
      selectedId: 'health',
      status: native ? 'unchecked' : 'preview',
      busy: false,
      validating: false,
      layoutDirty: false,
      error: null,
      notice: null,
      revision: 0,
      fileRevision: 0,
      inspectorOpen: true,
      inspectorTab: 'node',
      focusedFieldPath: null,
      setInspectorTab: (inspectorTab) => {
        if (Object.keys(get().formErrors).length) {
          return;
        }
        set({ inspectorTab, inspectorOpen: true, focusedFieldPath: null });
      },
      inspectDiagnostic: (diagnostic) => {
        if (Object.keys(get().formErrors).length) {
          set({ error: 'Apply or discard the current fields before opening a diagnostic.' });
          return;
        }
        const document = get().document;
        const match = diagnostic.fieldPath.match(/^\/workflows\/(\d+)/);
        const workflow = diagnostic.workflowId
          ? document?.workflows.find((workflow) => workflow.id === diagnostic.workflowId)
          : match
            ? document?.workflows[Number(match[1])]
            : undefined;
        const nodeIndex = diagnostic.fieldPath.match(/\/nodes\/(\d+)/);
        const edgeIndex = diagnostic.fieldPath.match(/\/edges\/(\d+)/);
        const edgeSource = workflow && edgeIndex ? workflow.edges[Number(edgeIndex[1])]?.from : null;
        const nodeId = workflow
          ? (diagnostic.nodeId ?? (nodeIndex ? workflow.nodes[Number(nodeIndex[1])]?.id : edgeSource))
          : diagnostic.workflowId
            ? null
            : diagnostic.nodeId;
        set({
          ...(workflow
            ? {
                selectedWorkflowId: workflow.id,
                canvasView:
                  document &&
                  phases.some(
                    (phase) => phaseWorkflow(document, phase)?.id === workflow.id && ownsPhase(document, phase)
                  )
                    ? ('lifecycle' as const)
                    : ('workflow' as const),
                selectedPhase: document
                  ? (phases.find((phase) => phaseWorkflow(document, phase)?.id === workflow.id) ?? get().selectedPhase)
                  : get().selectedPhase,
              }
            : {}),
          selectedId: nodeId ?? null,
          workflowTrail: [],
          inspectorOpen: true,
          inspectorTab: nodeId ? 'node' : workflow ? 'workflow' : 'scenario',
          focusedFieldPath: diagnostic.fieldPath,
        });
      },
      toggleInspector: () => {
        if (Object.keys(get().formErrors).length) {
          set({ error: 'Finish the current fields before closing the inspector.' });
          return;
        }
        set({ inspectorOpen: !get().inspectorOpen });
      },
      select: (selectedId) => {
        if (Object.keys(get().formErrors).length && selectedId !== get().selectedId) {
          set({ error: 'Finish the current fields or discard changes before selecting another node.' });
          return;
        }
        set({
          selectedId,
          ...(selectedId !== null
            ? { inspectorOpen: true, inspectorTab: 'node' as const, focusedFieldPath: null }
            : {}),
        });
      },
      move: (id, position) => {
        const document = get().document;
        if (!document || get().mode !== 'edit' || get().pendingChange) return;
        const moved = moveNode(document, id, position, get().selectedWorkflowId);
        if (moved === document) return;
        replace(moved, `move:${get().selectedWorkflowId}:${id}`);
        set({ layoutDirty: true });
      },
      loadExample: async () => {
        if (!native || !(await canReplace())) return;
        const revision = begin(true);
        try {
          const result = await bridge.validate(exampleSource, 'json');
          if (get().revision !== revision) return;
          reset(result.document);
          set({
            path: null,
            format: 'json',
            originalSource: exampleSource,
            layoutDirty: false,
            fileRevision: get().fileRevision + 1,
            selectedId: 'health',
          });
          apply(result, revision);
        } catch (error) {
          failed(error, revision);
        }
      },
      open: async () => {
        if (activeRun(useRunStore.getState().status) || get().pendingChange || get().saving) return;
        let revision = begin(true);
        try {
          const result = await bridge.open();
          if (get().revision !== revision) return;
          if (!result) {
            set({ busy: false, validating: false });
            return;
          }
          if (!result.validation.document) {
            set({
              diagnostics: result.validation.diagnostics,
              error: 'Cannot open this scenario. Review the file diagnostics.',
              busy: false,
              validating: false,
            });
            return;
          }
          set({ busy: false, validating: false });
          if (!(await canReplace())) return;
          revision = begin(true);
          reset(result.validation.document);
          register({
            key: result.path,
            path: result.path,
            name: result.validation.document.name,
            source: result.source,
            format: result.format,
          });
          set({
            mode: 'view',
            savePath: result.format === 'json' ? result.path : null,
            document: result.validation.document,
            path: result.path,
            format: result.format,
            originalSource: result.source,
            layoutDirty: false,
            selectedId:
              (result.validation.document ? iterationWorkflow(result.validation.document)?.nodes[0]?.id : null) ?? null,
            fileRevision: get().fileRevision + 1,
          });
          apply(result.validation, revision);
        } catch (error) {
          failed(error, revision);
        }
      },
      validate: async () => {
        const current = get();
        const source = current.document ? JSON.stringify(current.document) : current.originalSource;
        const format = current.document ? 'json' : current.format;
        const revision = begin(true);
        try {
          apply(await bridge.validate(source, format), revision, true);
        } catch (error) {
          failed(error, revision);
        }
      },
      save: async () => {
        const current = get();
        const document = current.document;
        if (
          current.mode !== 'edit' ||
          !current.dirty ||
          !document ||
          current.busy ||
          current.saving ||
          Object.keys(current.formErrors).length
        )
          return false;
        const revision = begin();
        set({ saving: true });
        try {
          const saved = await bridge.save(JSON.stringify(document), document.id, current.savePath);
          if (!saved) {
            if (get().revision === revision) set({ busy: false });
            return false;
          }
          const validation = typeof saved === 'object' ? ('validation' in saved ? saved.validation : saved) : null;
          const path = typeof saved === 'object' && 'path' in saved ? saved.path : current.savePath;
          if (get().entryKey !== current.entryKey || get().mode !== 'edit') return false;
          // Record the actual written baseline even when newer edits arrived during the write.
          newOrigin = null;
          baselineStatus = validation ? (validation.valid ? 'valid' : 'invalid') : current.status;
          baselineDiagnostics = validation?.diagnostics ?? current.diagnostics;
          set({
            savedDocument: JSON.stringify(document),
            savePath: path,
            dirty: JSON.stringify(get().document) !== JSON.stringify(document),
          });
          if (path)
            register({ key: path, path, name: document.name, source: JSON.stringify(document), format: 'json' });
          if (get().revision === revision)
            set({
              busy: false,
              ...(validation
                ? { diagnostics: validation.diagnostics, status: validation.valid ? 'valid' : 'invalid' }
                : {}),
              notice: validation && !validation.valid ? 'Saved with validation errors.' : 'Scenario saved.',
            });
          return get().revision === revision && !get().dirty;
        } catch (error) {
          failed(error, revision);
          return false;
        } finally {
          set({ saving: false });
        }
      },
    };
  });
}
export const useScenarioStore = createScenarioStore(
  { validate: validateSource, open: openScenarioFile, read: readScenarioFile, save: saveScenarioJson },
  desktopAvailable
);
