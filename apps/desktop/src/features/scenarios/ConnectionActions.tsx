import { localId, phaseWorkflow } from './lifecycle';
import { EdgeToolbar } from '@xyflow/react';
import { Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/button';
import { IconTooltip } from '../../components/ui/tooltip';
import { useScenarioStore } from './store';

export function ConnectionActions({ id, selected, x, y }: { id: string; selected?: boolean; x: number; y: number }) {
  const editable = useScenarioStore((state) => state.mode === 'edit' && !state.pendingChange && !state.busy);
  if (!selected || !editable) return null;
  return (
    <EdgeToolbar edgeId={id} x={x} y={y - 12} isVisible alignY="bottom">
      <div className="node-context-actions nodrag nopan" onClick={(event) => event.stopPropagation()}>
        <IconTooltip label="Delete connection">
          <Button
            size="icon"
            variant="ghost"
            aria-label="Delete connection"
            onClick={() => {
              const state = useScenarioStore.getState();
              const scoped = localId(id);
              const workflowId =
                scoped && state.document ? phaseWorkflow(state.document, scoped.phase)?.id : state.selectedWorkflowId;
              if (workflowId)
                state.command({
                  type: 'graph.delete',
                  workflowId,
                  nodeIds: [],
                  edgeIds: [scoped?.id ?? id],
                });
            }}
          >
            <Trash2 />
          </Button>
        </IconTooltip>
      </div>
    </EdgeToolbar>
  );
}
