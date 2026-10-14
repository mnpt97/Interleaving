import { useEffect, useId, useState } from "react";
import { Check, LoaderCircle, TriangleAlert } from "lucide-react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./ui/tabs";
import { ValidationPanel } from "../features/scenarios/ValidationPanel";
import { RunPanel } from "../features/runs/RunPanel";
import { useScenarioStore } from "../features/scenarios/store";

export function WorkspacePanels() {
  const [tab, setTab] = useState("validation");
  const status = useScenarioStore((state) => state.status);
  const validating = useScenarioStore((state) => state.validating);
  const fileRevision = useScenarioStore((state) => state.fileRevision);
  const statusId = useId();
  // New failed files must reveal their diagnostics even when the previous file
  // was also invalid. Successful loads preserve the user's selected panel.
  useEffect(() => { if (status === "invalid") setTab("validation"); }, [status, fileRevision]);
  return <Tabs value={tab} onValueChange={setTab} className="workspace-panels">
    <TabsList aria-label="Scenario panels">
      <TabsTrigger value="validation" aria-describedby={statusId}>{validating ? <LoaderCircle size={14} className="validation-tab-loading spin" aria-hidden="true" /> : status === "invalid" ? <TriangleAlert size={14} className="validation-tab-warning" aria-hidden="true" /> : status === "valid" ? <Check size={14} className="validation-tab-check" aria-hidden="true" /> : null}Validation</TabsTrigger>
      <TabsTrigger value="run">Run</TabsTrigger>
    </TabsList>
    <span id={statusId} className="sr-only">{validating ? "Validating scenario" : status === "invalid" ? "Validation failed" : status === "valid" ? "Validation passed" : "Not validated"}</span>
    <TabsContent value="validation"><ValidationPanel /></TabsContent>
    <TabsContent value="run"><RunPanel /></TabsContent>
  </Tabs>;
}
