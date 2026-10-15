import { Download, Play, Square } from 'lucide-react';
import { Button } from '../../components/ui/button';
import { desktopAvailable } from '../../lib/tauri';
import { useScenarioStore } from '../scenarios/store';
import { activeRun, useRunStore } from './store';
export function RunPanel() {
  const run = useRunStore();
  const scenario = useScenarioStore();
  const selected = run.events.find((event) => event.sequence === run.selected);
  const active = activeRun(run.status);
  return (
    <section className="run-panel" aria-label="HTTP run">
      <div className="panel-heading">
        <h2>HTTP run</h2>
        <span role="status">
          {run.status === 'idle' ? 'One user · sequential workflow' : run.status}
          {run.report
            ? ` · ${run.report.summary.responses}/${run.report.summary.requests} responses · ${(run.report.summary.elapsedNs / 1e6).toFixed(1)} ms`
            : ''}
        </span>
        <div className="panel-actions">
          <Button size="sm" variant="ghost" disabled={!desktopAvailable || !run.report} onClick={() => void run.save()}>
            <Download />
            Export report
          </Button>
          {active ? (
            <Button
              size="sm"
              variant="outline"
              disabled={!run.runId || run.status === 'cancelling'}
              onClick={() => void run.cancel()}
            >
              <Square />
              {run.status === 'cancelling' ? 'Cancelling…' : 'Cancel'}
            </Button>
          ) : (
            <Button
              size="sm"
              disabled={!desktopAvailable || !scenario.document || scenario.busy || scenario.status !== 'valid'}
              onClick={() => {
                if (scenario.document) void run.start(scenario.document);
              }}
            >
              <Play />
              Run
            </Button>
          )}
        </div>
      </div>
      {run.error && (
        <p className="run-error" role="alert">
          {run.error}
        </p>
      )}
      {run.notice && (
        <p className="run-notice" role="status">
          {run.notice}
        </p>
      )}
      {run.report?.eventsTruncated && (
        <p className="run-error">Report limit reached. Execution stopped; terminal events were retained.</p>
      )}
      <div className="run-content">
        {run.events.length ? (
          <>
            <ol className="run-timeline" aria-label="Run events">
              {run.events.map((event) => (
                <li key={event.sequence}>
                  <button aria-pressed={run.selected === event.sequence} onClick={() => run.select(event.sequence)}>
                    <time>{(event.elapsedNs / 1e6).toFixed(1)} ms</time>
                    <span>{event.type}</span>
                    <code>{event.nodeId ?? 'run'}</code>
                    {event.type === 'response.received' && <strong>HTTP {event.payload.status}</strong>}
                  </button>
                </li>
              ))}
            </ol>
            <div className="response-inspector" aria-label="Request and response inspector">
              {selected ? (
                <>
                  <h3>
                    {selected.type}
                    {selected.type === 'response.received' ? ` · HTTP ${selected.payload.status}` : ''}
                  </h3>
                  {selected.type === 'session.updated' && (
                    <section aria-label="Session changes">
                      <p>Only header names are recorded. Values stay in Rust.</p>
                      <dl className="details">
                        <div>
                          <dt>Set</dt>
                          <dd>{selected.payload.setHeaders.join(', ') || 'None'}</dd>
                        </div>
                        <div>
                          <dt>Removed</dt>
                          <dd>{selected.payload.removedHeaders.join(', ') || 'None'}</dd>
                        </div>
                      </dl>
                    </section>
                  )}
                  {(selected.type === 'request.sent' || selected.type === 'response.received') &&
                    selected.payload.body.omitted && (
                      <p className="payload-omission">
                        Payload omitted{selected.payload.body.reason ? ` (${selected.payload.body.reason})` : ''}.{' '}
                        {selected.payload.body.reason === 'Sensitive authentication payload omitted'
                          ? 'Authentication capture hides sensitive bodies and header values before they reach the UI.'
                          : 'This preview is unavailable; execution uses the configured capture limits.'}
                      </p>
                    )}
                  {selected.type === 'step.failed' && (
                    <p className="payload-omission">
                      {selected.payload.error.kind}: {selected.payload.error.message}
                    </p>
                  )}
                  <pre>{JSON.stringify(selected.payload, null, 2)}</pre>
                </>
              ) : (
                <p>Select an event to inspect its redacted details.</p>
              )}
            </div>
          </>
        ) : (
          <p className="run-empty">
            {desktopAvailable
              ? 'Run the validated scenario to send real HTTP requests. The trace shows redacted request and response details.'
              : 'Open the desktop app to execute HTTP requests. Browser preview cannot start runs.'}
          </p>
        )}
      </div>
    </section>
  );
}
