import { useEffect, useId, useState } from 'react';
import { Check, LoaderCircle, Play, TriangleAlert } from 'lucide-react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { ValidationPanel } from '../features/scenarios/ValidationPanel';
import { RunPanel } from '../features/runs/RunPanel';
import { useScenarioStore } from '../features/scenarios/store';
import { activeRun, useRunStore } from '../features/runs/store';

export function WorkspacePanels() {
  const [tab, setTab] = useState('validation');
  const status = useScenarioStore((state) => state.status);
  const validating = useScenarioStore((state) => state.validating);
  const fileRevision = useScenarioStore((state) => state.fileRevision);
  const statusId = useId();
  const runStatusId = useId();
  const runStatus = useRunStore((state) => state.status);
  const httpErrors = useRunStore(
    (state) =>
      (state.report?.summary.httpErrorResponses ?? 0) > 0 ||
      state.events.some((event) => event.type === 'response.received' && event.payload.status >= 400)
  );
  const running = activeRun(runStatus);
  const runWarning = runStatus === 'failed' || runStatus === 'error' || httpErrors;
  // New failed files must reveal their diagnostics even when the previous file
  // was also invalid. Successful loads preserve the user's selected panel.
  useEffect(() => {
    if (status === 'invalid') setTab('validation');
  }, [status, fileRevision]);
  return (
    <Tabs id="workspace-results" value={tab} onValueChange={setTab} className="workspace-panels">
      <TabsList aria-label="Scenario panels">
        <TabsTrigger value="validation" aria-describedby={statusId}>
          {validating ? (
            <LoaderCircle size={14} className="validation-tab-loading spin" aria-hidden="true" />
          ) : status === 'invalid' ? (
            <TriangleAlert size={14} className="validation-tab-warning" aria-hidden="true" />
          ) : status === 'valid' ? (
            <Check size={14} className="validation-tab-check" aria-hidden="true" />
          ) : null}
          Validation
        </TabsTrigger>
        <TabsTrigger value="run" aria-describedby={runStatusId}>
          {running ? (
            <LoaderCircle size={14} className="run-tab-loading spin" aria-hidden="true" />
          ) : runWarning ? (
            <TriangleAlert size={14} className="run-tab-warning" aria-hidden="true" />
          ) : (
            <Play size={14} className="run-tab-play" aria-hidden="true" />
          )}
          Run
        </TabsTrigger>
      </TabsList>
      <span id={statusId} className="sr-only">
        {validating
          ? 'Validating scenario'
          : status === 'invalid'
            ? 'Validation failed'
            : status === 'valid'
              ? 'Validation passed'
              : 'Not validated'}
      </span>
      <span id={runStatusId} className="sr-only">
        {running
          ? runStatus === 'cancelling'
            ? 'Cancelling run'
            : 'Run in progress'
          : runWarning
            ? 'Execution failed or returned HTTP errors'
            : runStatus === 'cancelled'
              ? 'Run cancelled'
              : runStatus === 'passed'
                ? 'Run completed'
                : 'Ready to run'}
      </span>
      <TabsContent value="validation">
        <ValidationPanel />
      </TabsContent>
      <TabsContent value="run">
        <RunPanel />
      </TabsContent>
    </Tabs>
  );
}
