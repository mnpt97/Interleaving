import { ArrowRightLeft } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '../../components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '../../components/ui/popover';
import { phaseWorkflow, phases, phaseName, canMovePhaseFields, type Phase } from './lifecycle';
import { useScenarioStore } from './store';

export function MoveToPhase({ phase, nodeId }: { phase: Phase; nodeId: string }) {
  const state = useScenarioStore();
  const [selected, setSelected] = useState([nodeId]);
  const [open, setOpen] = useState(false);
  useEffect(() => setSelected([nodeId]), [nodeId, open]);
  const workflow = state.document && phaseWorkflow(state.document, phase);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          size="icon"
          variant="ghost"
          aria-label="Move to phase"
          disabled={!state.document || !canMovePhaseFields(state.document, state.formErrors, phase, [nodeId])}
        >
          <ArrowRightLeft />
        </Button>
      </PopoverTrigger>
      <PopoverContent aria-label="Move nodes to phase">
        <fieldset className="phase-move-selection">
          <legend>Nodes</legend>
          {workflow?.nodes
            .filter((node) => node.type !== 'start' && node.type !== 'end')
            .map((node) => (
              <label key={node.id}>
                <input
                  type="checkbox"
                  checked={selected.includes(node.id)}
                  onChange={(event) =>
                    setSelected(event.target.checked ? [...selected, node.id] : selected.filter((id) => id !== node.id))
                  }
                />
                {node.name || node.id}
              </label>
            ))}
        </fieldset>
        <div className="panel-actions">
          {phases
            .filter((target) => target !== phase)
            .map((target) => (
              <Button
                key={target}
                size="sm"
                variant="outline"
                disabled={!selected.length}
                onClick={() => {
                  const destination = state.document && phaseWorkflow(state.document, target);
                  const empty = !destination?.nodes.some((node) => node.type !== 'start' && node.type !== 'end');
                  state.command({
                    type: 'phase.move',
                    from: phase,
                    to: target,
                    nodeIds: selected,
                    ...(empty && selected.length === 1 ? { position: { x: 120, y: 160 } } : {}),
                  });
                  if (!useScenarioStore.getState().error) setOpen(false);
                }}
              >
                {phaseName[target]}
              </Button>
            ))}
        </div>
      </PopoverContent>
    </Popover>
  );
}
