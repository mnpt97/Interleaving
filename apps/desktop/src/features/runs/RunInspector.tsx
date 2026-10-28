import { useRunView } from './useRunView';
import { detailEvent, nodeProgress } from './projection';
import { EventDetails, ExecutionDetails } from './RunDetails';
import { Button } from '../../components/ui/button';

export function RunInspector() {
  const { run, projection, workflow, invocation, node } = useRunView();
  const metadata = workflow?.nodes.find((value) => value.key === run.selectedNode);
  const event = node ? detailEvent(node, run.selected) : undefined;
  const children = projection.invocations.filter(
    (value) =>
      value.parentId === invocation?.id &&
      value.callerNodeKey === run.selectedNode &&
      value.groupKey === invocation?.groupKey &&
      value.userId === invocation?.userId &&
      value.phase === invocation?.phase &&
      value.iteration === invocation?.iteration
  );
  return (
    <aside className="inspector" aria-label="Run inspector">
      <h2>Run details</h2>
      <div className="inspector-content">
        {metadata ? (
          <>
            <h3>{metadata.name ?? metadata.id}</h3>
            <p className="muted">
              {invocation?.phase ?? 'Not executed'} · {invocation?.id ?? 'No invocation'}
            </p>
            {node ? (
              <>
                <ExecutionDetails node={node} />
                {node.error && (
                  <p role="alert" className="field-error">
                    {node.error}
                  </p>
                )}
                <EventDetails event={event} />
              </>
            ) : (
              <p>{nodeProgress(invocation, metadata.key)}</p>
            )}
            {children.map((child, index) => (
              <Button variant="ghost" size="sm" key={child.key} onClick={() => run.inspectInvocation(child.key)}>
                Open call {index + 1} ·{' '}
                {projection.graph?.workflows.find((workflow) => workflow.key === child.workflowKey)?.name}
              </Button>
            ))}
          </>
        ) : (
          <p className="muted">Select a node or execution.</p>
        )}
      </div>
    </aside>
  );
}
