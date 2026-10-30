import { Accordion, AccordionItem, AccordionTrigger, AccordionContent } from '../../components/ui/accordion';
import { phases, phaseName } from '../scenarios/lifecycle';
import { useRunView } from './useRunView';
import { nodeProgress } from './projection';
import { nodePresentation } from '../scenarios/node-presentation';
import { Button } from '../../components/ui/button';
import { TooltipProvider } from '../../components/ui/tooltip';
import { RunStatusIcon } from './RunStatusIcon';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../../components/ui/select';

export function RunSidebar() {
  const { run, projection, workflow, invocation } = useRunView();
  return (
    <TooltipProvider delayDuration={350}>
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
                  <RunStatusIcon status={projection.phases[phase] ?? (phaseWorkflow ? 'pending' : 'not configured')} />
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
                          <RunStatusIcon status={nodeProgress(root, node.key)} />
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
      <div className="run-sidebar-selectors">
        <div className="run-sidebar-field">
          <span id="run-workflow-label">Scenario / workflow</span>
          <Select
            value={run.canvasView === 'lifecycle' ? '__scenario__' : (workflow?.key ?? '')}
            onValueChange={(value) => (value === '__scenario__' ? run.showLifecycle() : run.inspectWorkflow(value))}
          >
            <SelectTrigger
              className="run-sidebar-select"
              aria-label="Selected run workflow"
              aria-labelledby="run-workflow-label"
            >
              <SelectValue placeholder="Select workflow" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="__scenario__">Main scenario</SelectItem>
              {projection.graph?.workflows.map((value) => (
                <SelectItem key={value.key} value={value.key}>
                  {value.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="run-sidebar-field">
          <span id="run-execution-label">Execution</span>
          <div className="run-invocation-picker">
            <Select
              value={invocation?.key ?? ''}
              onValueChange={(value) => run.inspectInvocation(value)}
              disabled={!projection.invocations.some((value) => value.workflowKey === workflow?.key)}
            >
              <SelectTrigger
                className="run-sidebar-select"
                aria-label="Selected invocation"
                aria-labelledby="run-execution-label"
              >
                <SelectValue placeholder="Not executed" />
              </SelectTrigger>
              <SelectContent>
                {projection.invocations
                  .filter((value) => value.workflowKey === workflow?.key)
                  .map((value, index) => (
                    <SelectItem key={value.key} value={value.key}>
                      {value.phase} · {value.userId ?? 'Unknown user'} · iteration {value.iteration ?? '—'} · call{' '}
                      {index + 1}
                    </SelectItem>
                  ))}
              </SelectContent>
            </Select>
            <RunStatusIcon status={invocation?.state ?? 'pending'} />
          </div>
        </div>
      </div>
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
                <RunStatusIcon status={nodeProgress(invocation, node.key)} />
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
    </TooltipProvider>
  );
}
