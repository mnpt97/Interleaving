import { phaseWorkflow, phaseName, ownsPhase, type Phase } from './lifecycle';
import { useEffect, useRef, useState } from 'react';
import {
  Globe,
  SquarePlus,
  KeyRound,
  ScanText,
  Workflow,
  ListFilter,
  CirclePlay,
  Flag,
  Diamond,
  X,
  Plus,
  ArrowLeft,
  Replace,
} from 'lucide-react';
import { Button } from '../../components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '../../components/ui/popover';
import { useScenarioStore } from './store';
import { selectedWorkflow } from '../../lib/scenario-types';
import { canCall, type InsertKind } from './graph';

export function AddNodePopover({
  afterNodeId,
  replaceNodeId,
  onExpandedChange,
  phase,
}: {
  phase?: Phase;
  afterNodeId?: string;
  replaceNodeId?: string;
  onExpandedChange?(expanded: boolean): void;
}) {
  const state = useScenarioStore();
  const [open, setOpen] = useState(false);
  const back = useRef<HTMLButtonElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const wasOpen = useRef(false);
  useEffect(() => {
    onExpandedChange?.(open);
    if (open) back.current?.focus();
    else if (wasOpen.current) trigger.current?.focus();
    wasOpen.current = open;
  }, [open, onExpandedChange]);
  const [callee, setCallee] = useState('__new__');
  const [workflowName, setWorkflowName] = useState('Workflow');
  const activePhase =
    phase ?? (state.canvasView === 'lifecycle' && !afterNodeId && !replaceNodeId ? state.selectedPhase : undefined);
  const workflow =
    state.document &&
    (activePhase
      ? phaseWorkflow(state.document, activePhase)
      : selectedWorkflow(state.document, state.selectedWorkflowId));
  if (state.mode !== 'edit' || (afterNodeId && !workflow?.nodes.some((node) => node.id === afterNodeId))) return null;
  const available =
    state.document?.workflows.filter((candidate) => !workflow || canCall(state.document!, workflow.id, candidate.id)) ??
    [];
  const insert = (kind: InsertKind) => {
    if (activePhase && !replaceNodeId && !afterNodeId) {
      state.command({
        type: 'phase.node.add',
        phase: activePhase,
        kind,
        ...(kind === 'call' ? (callee === '__new__' ? { newWorkflowName: workflowName } : { calleeId: callee }) : {}),
      });
      if (!useScenarioStore.getState().error) setOpen(false);
      return;
    }
    if (!workflow) return;
    if (replaceNodeId)
      state.command({
        type: 'node.change',
        workflowId: workflow.id,
        nodeId: replaceNodeId,
        kind,
        ...(kind === 'call' ? (callee === '__new__' ? { newWorkflowName: workflowName } : { calleeId: callee }) : {}),
      });
    else
      state.command({
        type: 'node.add',
        workflowId: workflow.id,
        kind,
        afterNodeId,
        ...(kind === 'call' ? (callee === '__new__' ? { newWorkflowName: workflowName } : { calleeId: callee }) : {}),
      });
    if (!useScenarioStore.getState().error) setOpen(false);
  };
  const disabled = Boolean(activePhase && state.document && !ownsPhase(state.document, activePhase));
  if (!open)
    return (
      <Button
        ref={trigger}
        size="icon"
        variant="ghost"
        aria-label={
          replaceNodeId
            ? 'Change node type'
            : afterNodeId
              ? 'Add after node'
              : phase
                ? `Add node to ${phaseName[phase]}`
                : 'Add node'
        }
        disabled={
          disabled ||
          (!workflow && !activePhase) ||
          state.busy ||
          state.pendingChange ||
          Boolean(Object.keys(state.formErrors).length)
        }
        onClick={() => {
          setOpen(true);
        }}
      >
        {replaceNodeId ? <Replace /> : <SquarePlus />}
      </Button>
    );
  return (
    <div
      className="node-add-picker"
      onKeyDown={(event) => {
        if (event.key === 'Escape' && !(event.target as HTMLElement).closest('[role="dialog"]')) {
          event.stopPropagation();
          setOpen(false);
        }
      }}
    >
      <div
        className="node-type-actions"
        role="group"
        aria-label={replaceNodeId ? 'Change node type' : 'Add node types'}
      >
        <Button
          ref={back}
          size="icon"
          variant="ghost"
          aria-label="Back"
          title="Back to node actions"
          onClick={() => setOpen(false)}
        >
          <ArrowLeft />
        </Button>
        {(['http', 'authentication', 'extract', 'setSession'] as const).map((kind) => {
          const { Icon, name } = {
            http: { Icon: Globe, name: 'HTTP Request' },
            authentication: { Icon: KeyRound, name: 'Authentication' },
            extract: { Icon: ScanText, name: 'Extract' },
            setSession: { Icon: ListFilter, name: 'Set session' },
          }[kind];
          return (
            <Button
              key={kind}
              size="icon"
              variant="ghost"
              aria-label={name}
              disabled={
                disabled ||
                Boolean(replaceNodeId && workflow?.nodes.find((node) => node.id === replaceNodeId)?.type === kind)
              }
              onClick={() => insert(kind)}
            >
              <Icon />
            </Button>
          );
        })}
        <Popover>
          <PopoverTrigger asChild>
            <Button
              size="icon"
              variant="ghost"
              aria-label="Gateway"
              title="Add a split / merge gateway"
              disabled={disabled}
            >
              <Diamond />
            </Button>
          </PopoverTrigger>
          <PopoverContent aria-label="Gateway types">
            <div className="node-type-actions" role="group" aria-label="Gateway types">
              <Button
                size="icon"
                variant="ghost"
                aria-label="Exclusive"
                title="Exclusive split / merge: choose one route"
                onClick={() => insert('exclusiveSplit')}
              >
                <X />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                aria-label="Parallel"
                title="Parallel split / merge is not available yet"
                disabled
              >
                <Plus />
              </Button>
            </div>
          </PopoverContent>
        </Popover>
        <Popover>
          <PopoverTrigger asChild>
            <Button size="icon" variant="ghost" aria-label="Call workflow">
              <Workflow />
            </Button>
          </PopoverTrigger>
          <PopoverContent aria-label="Call workflow">
            <label>
              Called workflow
              <select value={callee} onChange={(event) => setCallee(event.target.value)}>
                <option value="__new__">New reusable workflow</option>
                {available.map((candidate) => (
                  <option key={candidate.id} value={candidate.id}>
                    {candidate.name}
                  </option>
                ))}
              </select>
            </label>
            {callee === '__new__' && (
              <label>
                Workflow name
                <input value={workflowName} onChange={(event) => setWorkflowName(event.target.value)} maxLength={256} />
              </label>
            )}
            <Button
              size="icon"
              variant="ghost"
              aria-label="Insert workflow call"
              disabled={disabled || (callee === '__new__' && !workflowName.trim())}
              onClick={() => insert('call')}
            >
              <Workflow />
            </Button>
          </PopoverContent>
        </Popover>
      </div>
      {workflow &&
        !replaceNodeId &&
        !afterNodeId &&
        (['start', 'end'] as const)
          .filter((kind) => !workflow.nodes.some((node) => node.type === kind))
          .map((kind) => (
            <Button
              key={kind}
              size="icon"
              variant="ghost"
              aria-label={kind === 'start' ? 'Start' : 'End'}
              title={`Restore missing ${kind} node`}
              onClick={() => {
                state.command({ type: 'boundary.add', workflowId: workflow.id, kind });
                if (!useScenarioStore.getState().error) setOpen(false);
              }}
            >
              {kind === 'start' ? <CirclePlay size={18} /> : <Flag size={18} />}
            </Button>
          ))}
      {state.error && (
        <p role="alert" className="field-error">
          {state.error}
        </p>
      )}
    </div>
  );
}
