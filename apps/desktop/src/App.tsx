import { iterationWorkflow } from './lib/scenario-types';
import { lazy, Suspense, useEffect } from 'react';
import { CheckCircle2, FileJson2, FolderOpen, GitFork, PanelRightClose, PanelRightOpen } from 'lucide-react';
import { Button } from './components/ui/button';
import { ThemeControl } from './components/ThemeControl';
import { desktopAvailable } from './lib/tauri';
import { NodeInspector } from './features/scenarios/NodeInspector';
import { WorkspacePanels } from './components/WorkspacePanels';
import { useScenarioStore } from './features/scenarios/store';
import './App.css';
import { nodePresentation } from './features/scenarios/node-presentation';

const ScenarioCanvas = lazy(() =>
  import('./features/scenarios/ScenarioCanvas').then((module) => ({ default: module.ScenarioCanvas }))
);

function App() {
  const state = useScenarioStore();
  useEffect(() => {
    if (desktopAvailable) void useScenarioStore.getState().loadExample();
  }, []);
  const document = state.document;
  const workflow = document ? iterationWorkflow(document) : undefined;
  const available = desktopAvailable && !state.busy;
  const profile = document?.clientProfiles[0];
  return (
    <main className="app-shell">
      <header className="app-header">
        <div className="brand">
          <GitFork size={23} strokeWidth={1.8} />
          <span>Interleaving</span>
        </div>
        <span className="scope-note">Scenario workspace</span>
        <span className="local-label">Local files · schema v2</span>
        <ThemeControl />
      </header>
      <section className="scenario-toolbar" aria-label="Scenario actions">
        <div className="scenario-title">
          <h1>{document?.name ?? 'Scenario diagnostics'}</h1>
          <p title={state.path ?? undefined}>
            {state.path?.split(/[\\/]/).pop() ?? 'Example scenario'}
            <span> · {state.format.toUpperCase()}</span>
            {state.layoutDirty && <span> · Layout modified</span>}
          </p>
        </div>
        <div className="toolbar-actions">
          <Button variant="outline" disabled={!available} onClick={() => void state.open()}>
            <FolderOpen />
            Open scenario
          </Button>
          <Button
            variant="ghost"
            disabled={!available || !document || state.status === 'invalid'}
            onClick={() => void state.save()}
          >
            <FileJson2 />
            Save as JSON
          </Button>
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
      <div className={`workspace${state.inspectorOpen ? '' : ' inspector-collapsed'}`}>
        <aside className="scenario-sidebar">
          <h2>Workflow</h2>
          <nav aria-label="Workflow nodes">
            {workflow?.nodes.map((node, index) => {
              const { Icon, label } = nodePresentation[node.type];
              return (
                <button
                  key={node.id}
                  className={`node-nav ${node.id === state.selectedId ? 'active' : ''}`}
                  title={label}
                  aria-current={node.id === state.selectedId ? 'true' : undefined}
                  onClick={() => state.select(node.id)}
                >
                  <Icon size={16} />
                  <span>{node.id}</span>
                  <small>{index + 1}</small>
                </button>
              );
            })}
          </nav>
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
                <span>Identity: {group.identityAssignment.identityId ?? 'Anonymous'}</span>
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
              <p className="sidebar-note">
                Environment references resolve in Rust when you run. No credential values are loaded into this
                workspace.
              </p>
            </section>
          )}
          <p className="sidebar-note">
            Run one user’s HTTP and authentication workflow. Canvas layout is independent of execution order.
          </p>
        </aside>
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
        <div id="node-inspector" className="inspector-container" hidden={!state.inspectorOpen}>
          {state.inspectorOpen && <NodeInspector />}
        </div>
      </div>
      <WorkspacePanels />
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
