import { desktopHost } from './lib/desktop';
import { LifecycleSidebar } from './features/scenarios/LifecycleSidebar';
import { phases, ownsPhase } from './features/scenarios/lifecycle';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './components/ui/select';
import { RunSidebar } from './features/runs/RunSidebar';
import { useRunView } from './features/runs/useRunView';
import { useDraftValidation } from './features/scenarios/useDraftValidation';
import { ScenarioBaseUrl } from './features/scenarios/targets';
import { selectedWorkflow } from './lib/scenario-types';
import { lazy, Suspense, useEffect } from 'react';
import {
  CheckCircle2,
  Save,
  Undo2,
  Redo2,
  CirclePlus,
  FolderOpen,
  GitFork,
  PanelRightClose,
  PanelRightOpen,
} from 'lucide-react';
import { Button } from './components/ui/button';
import { ThemeControl } from './components/ThemeControl';
import { desktopAvailable } from './lib/desktop';
import { NodeInspector } from './features/scenarios/NodeInspector';
import { WorkspacePanels } from './components/WorkspacePanels';
import { PanelResizeHandle } from './components/PanelResizeHandle';
import { useWorkspaceLayout } from './components/useWorkspaceLayout';
import { useScenarioStore } from './features/scenarios/store';
import './App.css';

import { UnsavedChangesDialog } from './features/scenarios/UnsavedChangesDialog';
import { activeRun, useRunStore } from './features/runs/store';
import { presentationFor, nodeName } from './features/scenarios/node-presentation';

const AddNodePopover = lazy(() =>
  import('./features/scenarios/AddNodePopover').then((module) => ({ default: module.AddNodePopover }))
);

const ScenarioCanvas = lazy(() =>
  import('./features/scenarios/ScenarioCanvas').then((module) => ({ default: module.ScenarioCanvas }))
);

function App() {
  const state = useScenarioStore();
  useDraftValidation();
  const run = useRunStore();
  const runView = useRunView();
  const layout = useWorkspaceLayout(state.inspectorOpen);
  const editing = state.mode === 'edit';
  const stagedConfiguration = Object.keys(state.formErrors).some(
    (key) => key.startsWith('configuration:') || key.startsWith('fragment:') || key.startsWith('authentication:')
  );
  const running = activeRun(run.status);
  useEffect(() => {
    if (desktopAvailable) void useScenarioStore.getState().loadExample();
  }, []);
  useEffect(() => {
    const close = (event: BeforeUnloadEvent) => {
      const current = useScenarioStore.getState();
      if (current.dirty || Object.keys(current.formErrors).length) {
        event.preventDefault();
        event.returnValue = '';
      }
    };
    window.addEventListener('beforeunload', close);
    let unlisten: (() => void) | undefined;
    let disposed = false;
    if (desktopHost.native)
      void desktopHost
        .onCloseRequested(() => useScenarioStore.getState().requestClose())
        .then((stop) => {
          if (disposed) stop();
          else unlisten = stop;
        })
        .catch(() =>
          useScenarioStore.setState({ error: 'Close protection could not start. Save changes before closing.' })
        );
    return () => {
      disposed = true;
      unlisten?.();
      window.removeEventListener('beforeunload', close);
    };
  }, []);
  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      const current = useScenarioStore.getState();
      if (current.mode !== 'edit' || current.pendingChange || current.busy || !(event.metaKey || event.ctrlKey)) return;
      const field =
        event.target instanceof HTMLElement &&
        (event.target.closest('input, textarea, select, [role="combobox"], [role="listbox"], [role="option"]') ||
          event.target.isContentEditable);
      if (event.key.toLowerCase() === 's') {
        event.preventDefault();
        if (desktopAvailable) void current.save();
      }
      if (!field && event.key.toLowerCase() === 'z') {
        event.preventDefault();
        if (event.shiftKey) current.redo();
        else current.undo();
      }
      if (!field && event.key.toLowerCase() === 'y') {
        event.preventDefault();
        current.redo();
      }
    };
    window.addEventListener('keydown', keydown);
    return () => window.removeEventListener('keydown', keydown);
  }, []);
  const document = state.document;
  const workflow = document ? selectedWorkflow(document, state.selectedWorkflowId) : undefined;
  const available = desktopAvailable && !state.busy && !state.saving;
  const profile = document?.clientProfiles[0];
  return (
    <main className="app-shell">
      <header className="app-header">
        <div className="brand">
          <GitFork size={23} strokeWidth={1.8} />
          <span>Interleaving</span>
        </div>

        <span className="local-label">Local files</span>
        <ThemeControl />
      </header>
      <section className="scenario-toolbar" aria-label="Scenario actions">
        <div className="scenario-title">
          <h1>
            {editing && document ? (
              <input
                className="scenario-name"
                aria-label="Scenario name"
                value={document.name}
                onChange={(e) => state.command({ type: 'name', name: e.target.value }, 'scenario-name')}
                onBlur={state.endHistoryGroup}
              />
            ) : (
              <Select
                value={state.entryKey || ''}
                disabled={running || state.busy}
                onValueChange={(value) => void state.switchScenario(value)}
              >
                <SelectTrigger className="scenario-selector" aria-label="Scenario">
                  <SelectValue placeholder={document?.name ?? 'Select a scenario'}>
                    {state.entries.find((entry) => entry.key === state.entryKey)?.name ?? document?.name}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent>
                  {state.entries.map((entry) => (
                    <SelectItem key={entry.key} value={entry.key}>
                      {entry.name}
                      {entry.path ? ` · ${entry.path.split(/[\\/]/).pop()}` : ''}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          </h1>
          <p title={state.savePath ?? state.path ?? undefined}>
            {(state.savePath ?? state.path)?.split(/[\\/]/).pop() ??
              (state.entryKey ? 'Bundled scenario' : 'Unsaved scenario')}
            <span> · {state.format.toUpperCase()}</span>
            {state.dirty && <span> · Unsaved changes</span>}
          </p>
        </div>
        <div className="toolbar-actions">
          <div className="workspace-mode" role="group" aria-label="Workspace mode">
            <button aria-pressed={!editing} disabled={state.busy} onClick={() => void state.setMode('view')}>
              View/Run
            </button>
            <button
              aria-pressed={editing}
              disabled={state.busy || !document}
              onClick={() => void state.setMode('edit')}
            >
              Edit
            </button>
          </div>
          {editing ? (
            <div className="scenario-actions">
              <Suspense fallback={null}>
                <AddNodePopover />
              </Suspense>
              <Button
                size="icon"
                variant="ghost"
                aria-label="Save scenario"
                title="Save (⌘/Ctrl S)"
                disabled={!available || state.saving || !state.dirty || Boolean(Object.keys(state.formErrors).length)}
                onClick={() => void state.save()}
              >
                <Save />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                aria-label="Undo"
                title="Undo"
                disabled={!state.past.length || state.busy || stagedConfiguration}
                onClick={state.undo}
              >
                <Undo2 />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                aria-label="Redo"
                title="Redo"
                disabled={!state.future.length || state.busy || stagedConfiguration}
                onClick={state.redo}
              >
                <Redo2 />
              </Button>
            </div>
          ) : (
            <div className="scenario-actions gap-2">
              <Button
                size="icon"
                variant="ghost"
                aria-label="New scenario"
                title="New scenario"
                disabled={state.busy || running}
                onClick={() => void state.newScenario()}
              >
                <CirclePlus />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                aria-label="Open scenario"
                title="Open scenario"
                disabled={!available || running}
                onClick={() => void state.open()}
              >
                <FolderOpen />
              </Button>
            </div>
          )}
          <Button
            variant="ghost"
            aria-expanded={state.inspectorOpen}
            aria-controls="node-inspector"
            onClick={state.toggleInspector}
          >
            {state.inspectorOpen ? <PanelRightClose /> : <PanelRightOpen />}Inspector
          </Button>
        </div>
      </section>
      <div className="workspace-layout" ref={layout.ref} style={layout.style}>
        <div className={`workspace${state.inspectorOpen ? '' : ' inspector-collapsed'}`}>
          <aside id="workflow-sidebar" className="scenario-sidebar">
            {runView.viewing ? (
              <RunSidebar />
            ) : (
              <>
                <h2>Workflow</h2>
                <select
                  aria-label="Selected workflow"
                  value={state.canvasView === 'lifecycle' ? '__scenario__' : (workflow?.id ?? '')}
                  onChange={(e) =>
                    e.target.value === '__scenario__' ? state.showLifecycle() : state.selectWorkflow(e.target.value)
                  }
                >
                  <option value="__scenario__">Main scenario</option>
                  {document?.workflows
                    .filter(
                      (w) =>
                        !phases.some(
                          (phase) =>
                            document.groups[0]?.lifecycle[phase]?.workflowId === w.id && ownsPhase(document, phase)
                        )
                    )
                    .map((w) => (
                      <option key={w.id} value={w.id}>
                        {w.name}
                      </option>
                    ))}
                </select>
                {state.canvasView === 'lifecycle' ? (
                  <LifecycleSidebar key={`${state.entryKey}:${document?.id}`} />
                ) : (
                  <nav aria-label="Workflow nodes">
                    {(workflow?.nodes ?? []).map((node, index) => {
                      const { Icon, label } = presentationFor(node, document);
                      return (
                        <button
                          key={node.id}
                          className={`node-nav ${node.id === state.selectedId ? 'active' : ''}`}
                          title={label}
                          aria-current={node.id === state.selectedId ? 'true' : undefined}
                          onClick={() => state.select(node.id)}
                        >
                          <Icon size={16} />
                          <span>{nodeName(node, document)}</span>
                          <small>{index + 1}</small>
                        </button>
                      );
                    })}
                  </nav>
                )}
                <ScenarioBaseUrl />
                <section className="sidebar-section">
                  <h2>Client profiles</h2>
                  {document?.clientProfiles.map((client) => (
                    <div className="profile-summary" key={client.id}>
                      <strong>{client.id}</strong>
                      <code>{client.baseUrl}</code>
                      <span>{client.timeoutMs.toLocaleString()} ms timeout</span>
                    </div>
                  ))}
                </section>
                <section className="sidebar-section">
                  <h2>Virtual-user groups</h2>
                  {document?.groups.map((group) => (
                    <div className="group-summary" key={group.id}>
                      <strong>{group.id}</strong>
                      <span>1 user · once · {group.clientProfileId}</span>
                    </div>
                  ))}
                </section>
                {Boolean(document?.identities.length) && (
                  <section className="sidebar-section">
                    <h2>Identities</h2>
                    {document?.identities.map((identity) => (
                      <div className="profile-summary" key={identity.id}>
                        <strong>{identity.id}</strong>
                        {Object.entries(identity.credentials).map(([field, ref]) => (
                          <span key={field}>
                            {field}: <code>{ref.name}</code>
                          </span>
                        ))}
                      </div>
                    ))}
                  </section>
                )}
              </>
            )}
          </aside>
          <PanelResizeHandle
            label="Resize workflow panel"
            controls="workflow-sidebar"
            orientation="vertical"
            {...layout.workflow}
          />
          <section className="canvas-pane" aria-label="Workflow canvas">
            <div className="canvas-surface">
              <Suspense
                fallback={
                  <div className="canvas-empty" role="status">
                    Loading workflow…
                  </div>
                }
              >
                <ScenarioCanvas key={state.fileRevision} />
              </Suspense>
            </div>
          </section>
          {state.inspectorOpen && (
            <PanelResizeHandle
              label="Resize inspector"
              controls="node-inspector"
              orientation="vertical"
              reverse
              {...layout.inspector}
            />
          )}
          <div id="node-inspector" className="inspector-container" hidden={!state.inspectorOpen}>
            {state.inspectorOpen && <NodeInspector />}
          </div>
        </div>
        <PanelResizeHandle
          label="Resize Run and Validation panel"
          controls="workspace-results"
          orientation="horizontal"
          reverse
          {...layout.bottom}
        />
        <WorkspacePanels />
      </div>
      <UnsavedChangesDialog />
      <footer className="status-bar">
        <span className={state.error ? 'status-error' : ''} role={state.error ? 'alert' : 'status'}>
          {state.error ??
            state.notice ??
            (!desktopAvailable ? (
              'Browser preview — open the desktop app to validate and save files.'
            ) : state.busy ? (
              'Working…'
            ) : state.status === 'valid' ? (
              <>
                <CheckCircle2 size={13} />
                Validation passed
              </>
            ) : state.status === 'invalid' ? (
              'Validation failed — review the diagnostics above.'
            ) : (
              'Ready to validate'
            ))}
        </span>
        <span>
          {profile ? `${workflow?.nodes.length} nodes · ${workflow?.edges.length} connections` : 'No scenario loaded'}
        </span>
      </footer>
    </main>
  );
}

export default App;
