import { useId } from 'react';
import { EventDetails } from './RunDetails';
import { useRunView } from './useRunView';
import { detailEvent, duration, type NodeExecution, type Invocation, type Projection } from './projection';
import { Download, Play, Square, ChevronDown } from 'lucide-react';
import { Button } from '../../components/ui/button';
import { desktopAvailable } from '../../lib/desktop';
import { useScenarioStore } from '../scenarios/store';
import { activeRun, useRunStore } from './store';
export function RunPanel() {
  const run = useRunStore();
  const scenario = useScenarioStore();
  const { projection, viewing } = useRunView();
  const selected = run.events.find((event) => event.sequence === run.selected);
  const active = activeRun(run.status);
  return (
    <section className="run-panel" aria-label="HTTP run">
      <div className="panel-heading">
        <h2>HTTP run</h2>
        <span role="status">
          {run.status === 'idle' ? 'Not started' : run.status}
          {run.report
            ? ` · ${run.report.summary.responses}/${run.report.summary.requests} responses · ${(run.report.summary.elapsedNs / 1e6).toFixed(1)} ms`
            : ''}
        </span>
        <div className="panel-actions">
          {projection.graph && scenario.mode === 'view' && (
            <Button size="sm" variant="ghost" onClick={() => run.showSnapshot(!viewing)}>
              {viewing ? 'View scenario' : 'View run'}
            </Button>
          )}
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
              disabled={
                !desktopAvailable ||
                !scenario.document ||
                scenario.busy ||
                scenario.status !== 'valid' ||
                Object.keys(scenario.formErrors).length > 0
              }
              onClick={() => {
                void (async () => {
                  if (!(await scenario.setMode('view'))) return;
                  const current = useScenarioStore.getState();
                  if (current.document && !Object.keys(current.formErrors).length) await run.start(current.document);
                })();
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
      {(projection.incomplete || (!run.report && run.streamIncomplete)) && (
        <p className="run-notice" role="status">
          Some execution history is unavailable.
        </p>
      )}
      {Boolean(run.report?.deliveryDroppedEvents) && (
        <p className="run-notice">
          Live delivery dropped {run.report?.deliveryDroppedEvents} events; showing retained report history.
        </p>
      )}
      <div className="run-content">
        {run.events.length ? (
          <>
            <div className="run-timeline" aria-label="Node executions">
              {projection.invocations.map((invocation) => (
                <InvocationLog key={invocation.key} invocation={invocation} projection={projection} />
              ))}
              {projection.otherEvents.length > 0 && (
                <details className="run-system-events">
                  <summary>Run events</summary>
                  {projection.otherEvents.map((event) => (
                    <button key={event.sequence} onClick={() => run.select(event.sequence)}>
                      {event.type} · {(event.elapsedNs / 1e6).toFixed(1)} ms
                    </button>
                  ))}
                </details>
              )}
            </div>
            <div className="response-inspector" aria-label="Request and response inspector">
              <EventDetails event={selected} />
            </div>
          </>
        ) : (
          <p className="run-empty">
            {desktopAvailable ? 'Run a validated scenario to see results.' : 'Open the desktop app to run scenarios.'}
          </p>
        )}
      </div>
    </section>
  );
}
function InvocationLog({ invocation, projection }: { invocation: Invocation; projection: Projection }) {
  const run = useRunStore();
  const workflow = projection.graph?.workflows.find((value) => value.key === invocation.workflowKey);
  let depth = 0;
  let parent = invocation;
  while (parent.parentId && depth < 8) {
    const next = projection.invocations.find(
      (value) =>
        value.id === parent.parentId &&
        value.groupKey === invocation.groupKey &&
        value.userId === invocation.userId &&
        value.phase === invocation.phase &&
        value.iteration === invocation.iteration
    );
    if (!next) break;
    parent = next;
    depth++;
  }
  return (
    <section
      className="run-invocation"
      style={{ paddingInlineStart: depth * 12 }}
      aria-label={`${workflow?.name ?? 'Workflow'} ${invocation.phase ?? 'execution'} ${invocation.id ?? 'uncorrelated'}`}
    >
      <h3>
        <button onClick={() => run.inspectInvocation(invocation.key)}>
          {workflow?.name ?? 'Workflow'}
          <span>
            {invocation.phase ?? 'Uncorrelated'} · {invocation.id ?? 'Unknown invocation'} · {invocation.state}
          </span>
        </button>
      </h3>
      {invocation.nodes.map((node) => (
        <ExecutionRow
          key={node.key}
          node={node}
          name={
            workflow?.nodes.find((value) => value.key === node.nodeKey)?.name ??
            workflow?.nodes.find((value) => value.key === node.nodeKey)?.id ??
            node.nodeId
          }
        />
      ))}
    </section>
  );
}
function ExecutionRow({ node, name }: { node: NodeExecution; name: string }) {
  const run = useRunStore();
  const id = useId();
  const expanded = Boolean(run.expanded[node.key]);
  return (
    <div className={`execution-row state-${node.state}`}>
      <button
        className="execution-summary"
        aria-expanded={expanded}
        aria-controls={id}
        onClick={() => {
          run.toggleExecution(node.key);
          const latest = detailEvent(node);
          if (latest) run.select(latest.sequence);
        }}
      >
        <ChevronDown size={14} className={expanded ? 'is-expanded' : ''} />
        <strong>{name}</strong>
        <span className="execution-state">{node.state}</span>
        <time>{duration(node)}</time>
        {node.httpStatus !== undefined && <span>HTTP {node.httpStatus}</span>}
      </button>
      {node.error && (
        <p className="execution-error" role="alert">
          {node.error}
        </p>
      )}
      {node.route && <p className="execution-route">{node.route}</p>}
      <div id={id} hidden={!expanded} className="execution-events">
        {node.events
          .filter((event) => event.type !== 'route.selected')
          .map((event) => (
            <button
              key={event.sequence}
              aria-pressed={run.selected === event.sequence}
              onClick={() => run.select(event.sequence)}
            >
              <time>{(event.elapsedNs / 1e6).toFixed(1)} ms</time>
              <span>{event.type}</span>
            </button>
          ))}
      </div>
    </div>
  );
}
