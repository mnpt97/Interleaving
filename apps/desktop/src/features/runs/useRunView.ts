import { useScenarioStore } from '../scenarios/store';
import { useMemo } from 'react';
import { useRunStore } from './store';
import { projectRun, type Projection } from './projection';
import type { RunEvent, RunReport } from '../../lib/run-types';

const cache = new WeakMap<
  RunEvent[],
  { report: RunReport | null; taskStatus?: RunReport['status']; projection: Projection }
>();
function cachedProjection(events: RunEvent[], report: RunReport | null, taskStatus?: RunReport['status']) {
  const existing = cache.get(events);
  if (existing?.report === report && existing.taskStatus === taskStatus) return existing.projection;
  const projection = projectRun(events, report, taskStatus);
  cache.set(events, { report, taskStatus, projection });
  return projection;
}

export function useRunView() {
  const run = useRunStore();
  const mode = useScenarioStore((state) => state.mode);
  const taskStatus =
    run.status === 'passed' || run.status === 'failed' || run.status === 'cancelled' || run.status === 'error'
      ? run.status
      : undefined;
  const projection = useMemo(
    () => cachedProjection(run.events, run.report, taskStatus),
    [run.events, run.report, taskStatus]
  );
  let invocation =
    projection.invocations.find((value) => value.key === run.selectedInvocation) ??
    (!run.selectedWorkflow
      ? ([...projection.invocations].reverse().find((value) => value.state === 'active') ??
        projection.invocations.find((value) => value.phase === 'iteration' && !value.parentId) ??
        projection.invocations[projection.invocations.length - 1])
      : undefined);
  if (run.canvasView === 'lifecycle' && invocation?.parentId) {
    const active = invocation;
    invocation =
      projection.invocations.find(
        (value) =>
          !value.parentId &&
          value.phase === active.phase &&
          value.groupKey === active.groupKey &&
          value.userId === active.userId &&
          value.iteration === active.iteration
      ) ?? invocation;
  }
  const workflow =
    projection.graph?.workflows.find(
      (workflow) => workflow.key === (run.selectedWorkflow ?? invocation?.workflowKey)
    ) ?? projection.graph?.workflows[0];
  const node = [...(invocation?.nodes ?? [])]
    .reverse()
    .find((node) => (run.selectedExecution ? node.key === run.selectedExecution : node.nodeKey === run.selectedNode));
  return {
    run,
    projection,
    invocation,
    workflow,
    node,
    viewing: mode === 'view' && run.showRun && Boolean(projection.graph),
  };
}
