import { lazy, Suspense, useEffect } from "react";
import { CheckCircle2, CirclePlay, FileJson2, Flag, FolderOpen, GitFork, Globe } from "lucide-react";
import { Button } from "./components/ui/button";
import { ThemeControl } from "./components/ThemeControl";
import { desktopAvailable } from "./lib/tauri";
import { NodeInspector } from "./features/scenarios/NodeInspector";
import { WorkspacePanels } from "./components/WorkspacePanels";
import { useScenarioStore } from "./features/scenarios/store";
import "./App.css";

const ScenarioCanvas = lazy(() => import("./features/scenarios/ScenarioCanvas").then((module) => ({ default: module.ScenarioCanvas })));

function App() {
  const state = useScenarioStore();
  useEffect(() => { if (desktopAvailable) void useScenarioStore.getState().loadExample(); }, []);
  const document = state.document;
  const available = desktopAvailable && !state.busy;
  const profile = document?.clientProfiles[0];
  return <main className="app-shell">
    <header className="app-header"><div className="brand"><GitFork size={23} strokeWidth={1.8} /><span>Interleaving</span></div><span className="scope-note">Scenario workspace</span><span className="local-label">Local files · schema v1</span><ThemeControl /></header>
    <section className="scenario-toolbar" aria-label="Scenario actions"><div className="scenario-title"><h1>{document?.name ?? "Scenario diagnostics"}</h1><p title={state.path ?? undefined}>{state.path?.split(/[\\/]/).pop() ?? "Example scenario"}<span> · {state.format.toUpperCase()}</span>{state.layoutDirty && <span> · Layout modified</span>}</p></div><div className="toolbar-actions">
      <Button variant="outline" disabled={!available} onClick={() => void state.open()}><FolderOpen />Open scenario</Button>
      <Button variant="ghost" disabled={!available || !document || state.status === "invalid"} onClick={() => void state.save()}><FileJson2 />Save as JSON</Button>
    </div></section>
    <div className="workspace">
      <aside className="scenario-sidebar"><h2>Workflow</h2><nav aria-label="Workflow nodes">{document?.workflow.nodes.map((node, index) => { const Icon = node.type === "http" ? Globe : node.type === "start" ? CirclePlay : Flag; return <button key={node.id} className={`node-nav ${node.id === state.selectedId ? "active" : ""}`} aria-current={node.id === state.selectedId ? "true" : undefined} onClick={() => state.select(node.id)}><Icon size={16} /><span>{node.id}</span><small>{index + 1}</small></button>; })}</nav>
        <section className="sidebar-section"><h2>Client profiles</h2>{document?.clientProfiles.map((client) => <div className="profile-summary" key={client.id}><strong>{client.id}</strong><code>{client.baseUrl}</code><span>{client.timeoutMs.toLocaleString()} ms timeout</span></div>)}</section>
        <section className="sidebar-section"><h2>Virtual-user groups</h2>{document?.groups.map((group) => <div className="group-summary" key={group.id}><strong>{group.id}</strong><span>{group.count} {group.count === 1 ? "user" : "users"} · {group.clientProfileId}</span></div>)}</section>
        <p className="sidebar-note">Run one anonymous user’s HTTP workflow. Canvas layout is independent of execution order.</p>
      </aside>
      <section className="canvas-pane" aria-label="Workflow canvas"><div className="canvas-surface"><Suspense fallback={<div className="canvas-empty" role="status">Loading workflow…</div>}><ScenarioCanvas key={state.fileRevision} /></Suspense></div></section>
      <NodeInspector />
    </div>
    <WorkspacePanels />
    <footer className="status-bar"><span className={state.error ? "status-error" : ""} role={state.error ? "alert" : "status"}>{state.error ?? state.notice ?? (!desktopAvailable ? "Browser preview — open the desktop app to validate and save files." : state.busy ? "Working…" : state.status === "valid" ? <><CheckCircle2 size={13} />Validation passed</> : state.status === "invalid" ? "Validation failed — review the diagnostics above." : "Ready to validate")}</span><span>{profile ? `${document?.workflow.nodes.length} nodes · ${document?.workflow.edges.length} connections` : "No scenario loaded"}</span></footer>
  </main>;
}

export default App;
