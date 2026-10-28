import { Accordion, AccordionItem, AccordionTrigger, AccordionContent } from '../../components/ui/accordion';
import { phases, phaseName } from '../scenarios/lifecycle';
import { useRunView } from './useRunView';
import { nodeProgress } from './projection';
import { nodePresentation } from '../scenarios/node-presentation';
import { Button } from '../../components/ui/button';

export function RunSidebar() {
  const { run, projection, workflow, invocation } = useRunView();
  return (
    <>
      <h2>Workflow</h2>
      {(invocation?.parentId || run.canvasView === 'invocation') && (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            if (!invocation) {
              run.showLifecycle();
              return;
            }
            const parent = projection.invocations.find(
              (value) =>
                value.id === invocation.parentId &&
                value.groupKey === invocation.groupKey &&
                value.userId === invocation.userId &&
                value.phase === invocation.phase &&
                value.iteration === invocation.iteration
            );
            if (parent) run.inspectInvocation(parent.key);
            else run.showLifecycle();
          }}
        >
          {invocation?.parentId ? 'Back to caller' : 'Back to scenario'}
        </Button>
      )}
      <nav className="lifecycle-accordion" aria-label="Run lifecycle">
        <Accordion type="multiple" defaultValue={['iteration']}>
          {phases.map((phase) => {
            const group =
              projection.graph?.groups.find((group) => group.key === invocation?.groupKey) ??
              projection.graph?.groups[0];
            const phaseWorkflow = projection.graph?.workflows.find((workflow) => workflow.key === group?.[phase]);
            const root = [...projection.invocations]
              .reverse()
              .find(
                (value) =>
                  value.phase === phase &&
                  !value.parentId &&
                  value.groupKey === group?.key &&
                  (!invocation?.userId || value.userId === invocation.userId)
              );
            const nodes = phaseWorkflow?.nodes.filter((node) => node.type !== 'start' && node.type !== 'end') ?? [];
            return (
              <AccordionItem key={phase} value={phase}>
                <AccordionTrigger>
                  <span>{phaseName[phase]}</span>{' '}
                  <small>{projection.phases[phase] ?? (phaseWorkflow ? 'pending' : 'not configured')}</small>
                </AccordionTrigger>
                <AccordionContent>
                  {nodes.length ? (
                    nodes.map((node) => {
                      const { Icon } = nodePresentation[node.type];
                      const selected = run.selectedNode === node.key && invocation?.phase === phase;
                      return (
                        <button
                          key={node.key}
                          className={`node-nav ${selected ? 'active' : ''}`}
                          disabled={!root}
                          aria-current={selected ? 'true' : undefined}
                          onClick={() => root && run.inspectNode(node.key, root.key)}
                        >
                          <Icon size={16} />
                          <span>{node.name ?? node.id}</span>
                          <small>{nodeProgress(root, node.key)}</small>
                        </button>
                      );
                    })
                  ) : (
                    <span className="accordion-empty">No steps</span>
                  )}
                </AccordionContent>
              </AccordionItem>
            );
          })}
        </Accordion>
      </nav>
      <label>
        <select
          aria-label="Selected run workflow"
          value={run.canvasView === 'lifecycle' ? '__scenario__' : (workflow?.key ?? '')}
          onChange={(event) =>
            event.target.value === '__scenario__' ? run.showLifecycle() : run.inspectWorkflow(event.target.value)
          }
        >
          <option value="__scenario__">Main scenario</option>
          {projection.graph?.workflows.map((value) => (
            <option key={value.key} value={value.key}>
              {value.name}
            </option>
          ))}
        </select>
      </label>
      <label>
        Execution
        <select
          aria-label="Selected invocation"
          value={invocation?.key ?? ''}
          onChange={(event) => run.inspectInvocation(event.target.value)}
        >
          <option value="" disabled>
            Not executed
          </option>
          {projection.invocations
            .filter((value) => value.workflowKey === workflow?.key)
            .map((value, index) => (
              <option key={value.key} value={value.key}>
                {value.phase} · {value.userId ?? 'Unknown user'} · iteration {value.iteration ?? '—'} · call {index + 1}{' '}
                · {value.state}
              </option>
            ))}
        </select>
      </label>
      {run.canvasView === 'invocation' && (
        <nav aria-label="Run nodes">
          {workflow?.nodes.map((node) => {
            const { Icon } = nodePresentation[node.type];
            return (
              <button
                key={node.key}
                className={`node-nav ${run.selectedNode === node.key ? 'active' : ''}`}
                aria-current={run.selectedNode === node.key ? 'true' : undefined}
                onClick={() => run.inspectNode(node.key, invocation?.key)}
              >
                <Icon size={16} />
                <span>{node.name ?? node.id}</span>
                <small>{nodeProgress(invocation, node.key)}</small>
              </button>
            );
          })}
        </nav>
      )}
      <section className="sidebar-section">
        <h2>Workload</h2>
        {projection.graph?.groups.map((group) => (
          <div className="group-summary" key={group.key}>
            <strong>{group.id}</strong>
            <span>{invocation?.userId ?? 'No user selected'}</span>
          </div>
        ))}
      </section>
    </>
  );
}
