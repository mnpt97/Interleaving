import { extractionOutputs } from './extraction';
import { Handle, NodeToolbar, Position, useUpdateNodeInternals, type NodeProps } from '@xyflow/react';
import { authenticationDefinition } from './authentication';
import { presentationFor, nodeName } from './node-presentation';
import { useEffect, useState } from 'react';
import { Trash2, ExternalLink, Play, FlagTriangleRight, X } from 'lucide-react';
import { Button } from '../../components/ui/button';
import { AddNodePopover } from './AddNodePopover';
import { MoveToPhase } from './MoveToPhase';
import type { ScenarioFlowNode } from './adapters';
import { useScenarioStore } from './store';

export function ScenarioNode({ id, data, selected }: NodeProps<ScenarioFlowNode>) {
  const node = data.node;
  const updateNodeInternals = useUpdateNodeInternals();
  useEffect(() => {
    updateNodeInternals(id ?? node.id);
  }, [id, node.id, node.type, updateNodeInternals]);
  const [adding, setAdding] = useState(false);
  const [changing, setChanging] = useState(false);
  useEffect(() => {
    setAdding(false);
    setChanging(false);
  }, [node.type]);
  useEffect(() => {
    if (!selected) {
      setAdding(false);
      setChanging(false);
    }
  }, [selected]);
  const boundary = node.type === 'start' || node.type === 'end';
  const gateway = node.type === 'exclusiveSplit' || node.type === 'exclusiveMerge';
  const BoundaryIcon = node.type === 'start' ? Play : FlagTriangleRight;
  const mode = useScenarioStore((s) => s.mode);
  const draft = useScenarioStore((s) => s.document);
  const document = draft;
  const { Icon, label } = presentationFor(node, document);
  const authentication = document ? authenticationDefinition(document, node) : null;
  const login = authentication?.nodes.find((n) => n.type === 'http');
  return (
    <div
      className={`scenario-node ${boundary ? `scenario-node-boundary scenario-node-${node.type}` : gateway ? 'scenario-node-gateway' : ''} ${selected ? 'is-selected' : ''}`}
      title={boundary || gateway ? `${label}: ${node.id}` : undefined}
    >
      {selected && mode === 'edit' && (
        <NodeToolbar isVisible position={Position.Top}>
          <div className="node-context-actions nodrag nopan" onClick={(event) => event.stopPropagation()}>
            {!changing && node.type !== 'end' && <AddNodePopover afterNodeId={node.id} onExpandedChange={setAdding} />}
            {!adding && !boundary && <AddNodePopover replaceNodeId={node.id} onExpandedChange={setChanging} />}
            {!adding && !changing && data.phase && !boundary && <MoveToPhase phase={data.phase} nodeId={node.id} />}
            {!adding && !changing && !['start', 'end'].includes(node.type) && (
              <Button
                size="icon"
                variant="ghost"
                aria-label="Delete node"
                title="Delete node and attached connections (undoable)"
                onClick={() => {
                  const state = useScenarioStore.getState();
                  if (state.selectedWorkflowId)
                    state.command({
                      type: 'graph.delete',
                      workflowId: state.selectedWorkflowId,
                      nodeIds: [node.id],
                      edgeIds: [],
                    });
                }}
              >
                <Trash2 />
              </Button>
            )}
          </div>
        </NodeToolbar>
      )}
      {(
        [
          ['left', Position.Left],
          ['right', Position.Right],
          ['top', Position.Top],
          ['bottom', Position.Bottom],
        ] as const
      ).flatMap(([side, position]) => [
        ...(node.type !== 'end'
          ? [
              <Handle
                key={`${side}-source`}
                id={side}
                type="source"
                position={position}
                aria-label={`${side} output`}
                style={{ visibility: mode === 'edit' ? 'visible' : 'hidden' }}
                isConnectable={mode === 'edit'}
                isConnectableEnd={false}
              />,
            ]
          : []),
        ...(node.type !== 'start'
          ? [
              <Handle
                key={`${side}-target`}
                id={side}
                type="target"
                position={position}
                aria-label={`${side} input`}
                style={{ visibility: mode === 'edit' ? 'visible' : 'hidden' }}
                isConnectable={mode === 'edit'}
                isConnectableStart={false}
              />,
            ]
          : []),
      ])}
      {gateway ? (
        <X className="gateway-symbol" size={24} aria-label="Exclusive gateway" />
      ) : boundary ? (
        <BoundaryIcon size={24} aria-label={label} />
      ) : (
        <>
          <div className="node-heading">
            <Icon size={16} />
            <span>{label}</span>
          </div>
          <strong>{nodeName(node, document)}</strong>
        </>
      )}

      {(node.type === 'http' || node.type === 'authentication') && (
        <div className="request-summary">
          <span>{node.config.method}</span>
          <code>{node.config.path}</code>
        </div>
      )}
      {node.type === 'call' && (
        <p className="auth-node-summary">
          {login?.type === 'http' ? `${login.config.method} ${login.config.path}` : node.config.workflowId}
        </p>
      )}
      {node.type === 'extract' && (
        <p className="auth-node-summary">
          {node.config.sourceNodeId} → {Object.keys(extractionOutputs(node.config)).join(', ')}
        </p>
      )}
      {node.type === 'setSession' && (
        <p className="auth-node-summary">
          {Object.keys(node.config.setHeaders).length} set · {node.config.removeHeaders.length} removed
        </p>
      )}
      {node.type === 'call' && document && (
        <details className="call-preview nodrag nopan nowheel" onClick={(event) => event.stopPropagation()}>
          <summary>Preview workflow</summary>
          <ul>
            {document.workflows
              .find((workflow) => workflow.id === node.config.workflowId)
              ?.nodes.map((child) => (
                <li key={child.id}>
                  {presentationFor(child, document).label} · {nodeName(child, document)}
                </li>
              ))}
          </ul>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              const state = useScenarioStore.getState();
              if (data.phase) state.selectPhase(data.phase, node.id);
              useScenarioStore.getState().enterWorkflow(node.id);
            }}
          >
            <ExternalLink />
            Open workflow
          </Button>
        </details>
      )}
    </div>
  );
}
