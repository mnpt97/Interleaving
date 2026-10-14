import { CheckCircle2, CircleAlert, LoaderCircle, ShieldCheck } from "lucide-react";
import { Button } from "../../components/ui/button";
import { desktopAvailable } from "../../lib/tauri";
import { useScenarioStore } from "./store";

export function ValidationPanel() {
  const diagnostics = useScenarioStore((s) => s.diagnostics);
  const status = useScenarioStore((s) => s.status);
  const select = useScenarioStore((s) => s.select);
  const busy = useScenarioStore((s) => s.busy);
  const validate = useScenarioStore((s) => s.validate);
  return <section className="validation-panel" aria-label="Validation diagnostics"><div className="panel-heading"><h2>Validation</h2><span>{diagnostics.length ? `${diagnostics.length} diagnostic${diagnostics.length === 1 ? "" : "s"}` : status === "valid" ? "No issues" : "Not validated"}</span><div className="panel-actions"><Button size="sm" disabled={!desktopAvailable || busy} onClick={() => void validate()}>{busy ? <LoaderCircle className="spin" /> : <ShieldCheck />}Validate</Button></div></div>
    <div className="diagnostics" aria-live="polite">{diagnostics.length ? diagnostics.map((diagnostic, index) => <button className="diagnostic" key={`${diagnostic.code}-${index}`} onClick={() => select(diagnostic.nodeId)}><CircleAlert size={16} /><span><strong>{diagnostic.message}</strong><small>{diagnostic.code} · {diagnostic.fieldPath}{diagnostic.line ? ` · line ${diagnostic.line}, column ${diagnostic.column}` : ""}</small></span></button>) : <p className="validation-empty">{status === "valid" ? <><CheckCircle2 size={16} />Scenario schema and workflow are valid.</> : "Validate in the desktop app to check the schema, references, and workflow."}</p>}</div>
  </section>;
}
