import { useEffect, useState } from 'react';
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '../../components/ui/accordion';
import { ownsPhase, phaseWorkflow, phases, phaseName } from './lifecycle';
import { presentationFor, nodeName } from './node-presentation';
import { useScenarioStore } from './store';

export function LifecycleSidebar() {
  const state = useScenarioStore();
  const [expanded, setExpanded] = useState<string[]>(['iteration']);
  useEffect(() => {
    if (state.selectedId)
      setExpanded((current) => (current.includes(state.selectedPhase) ? current : [...current, state.selectedPhase]));
  }, [state.selectedPhase, state.selectedId]);
  const document = state.document;
  if (!document) return null;
  return (
    <nav aria-label="Lifecycle phases" className="lifecycle-accordion">
      <Accordion type="multiple" value={expanded} onValueChange={setExpanded}>
        {phases.map((phase) => {
          const workflow = phaseWorkflow(document, phase);
          const nodes = workflow?.nodes.filter((node) => node.type !== 'start' && node.type !== 'end') ?? [];
          return (
            <AccordionItem key={phase} value={phase}>
              <AccordionTrigger>{phaseName[phase]}</AccordionTrigger>
              <AccordionContent>
                {nodes.length ? (
                  nodes.map((node, index) => {
                    const { Icon, label } = presentationFor(node, document);
                    const selected = state.selectedPhase === phase && state.selectedId === node.id;
                    return (
                      <button
                        key={node.id}
                        className={`node-nav ${selected ? 'active' : ''}`}
                        title={label}
                        aria-current={selected ? 'true' : undefined}
                        onClick={() => {
                          if (ownsPhase(document, phase)) state.selectPhase(phase, node.id);
                          else if (workflow) {
                            state.selectWorkflow(workflow.id);
                            if (useScenarioStore.getState().selectedWorkflowId === workflow.id)
                              useScenarioStore.getState().select(node.id);
                          }
                        }}
                      >
                        <Icon size={16} />
                        <span>{nodeName(node, document)}</span>
                        <small>{index + 1}</small>
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
  );
}
