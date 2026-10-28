import { Handle, Position, type NodeProps, type Node } from '@xyflow/react';
import { Play, FlagTriangleRight, X } from 'lucide-react';
import { nodePresentation } from '../scenarios/node-presentation';
import type { WorkflowNode } from '../../lib/scenario-types';
import type { Progress, RunGraph } from './projection';

export type CapturedNode = Node<
  { node: RunGraph['workflows'][number]['nodes'][number]; state: Progress; httpStatus?: number; port?: boolean },
  'captured'
>;
export function RunNode({ data, selected }: NodeProps<CapturedNode>) {
  const node = data.node;
  const boundary = node.type === 'start' || node.type === 'end';
  const gateway = node.type === 'exclusiveSplit' || node.type === 'exclusiveMerge';
  const { Icon, label } = nodePresentation[node.type as WorkflowNode['type']];
  if (data.port)
    return (
      <div className={`phase-port state-${data.state}`}>
        {(['left', 'right', 'top', 'bottom'] as const).map((side) => (
          <Handle
            key={side}
            id={side}
            type={node.type === 'start' ? 'source' : 'target'}
            position={
              Position[side === 'left' ? 'Left' : side === 'right' ? 'Right' : side === 'top' ? 'Top' : 'Bottom']
            }
            isConnectable={false}
            style={{ visibility: 'hidden' }}
          />
        ))}
      </div>
    );
  return (
    <div
      className={`scenario-node run-node state-${data.state} ${boundary ? `scenario-node-boundary scenario-node-${node.type}` : gateway ? 'scenario-node-gateway' : ''} ${selected ? 'is-selected' : ''}`}
      data-progress={data.state}
    >
      {(['left', 'right', 'top', 'bottom'] as const).flatMap((side) => [
        ...(node.type !== 'end'
          ? [
              <Handle
                key={`${side}-out`}
                id={side}
                type="source"
                position={
                  Position[side === 'left' ? 'Left' : side === 'right' ? 'Right' : side === 'top' ? 'Top' : 'Bottom']
                }
                style={{ visibility: 'hidden' }}
                isConnectable={false}
              />,
            ]
          : []),
        ...(node.type !== 'start'
          ? [
              <Handle
                key={`${side}-in`}
                id={side}
                type="target"
                position={
                  Position[side === 'left' ? 'Left' : side === 'right' ? 'Right' : side === 'top' ? 'Top' : 'Bottom']
                }
                style={{ visibility: 'hidden' }}
                isConnectable={false}
              />,
            ]
          : []),
      ])}
      {gateway ? (
        <X className="gateway-symbol" aria-label={label} />
      ) : boundary ? (
        node.type === 'start' ? (
          <Play aria-label="Start" />
        ) : (
          <FlagTriangleRight aria-label="End" />
        )
      ) : (
        <>
          <div className="node-heading">
            <Icon size={16} />
            <span>{label}</span>
          </div>
          <strong>{node.name ?? node.id}</strong>
        </>
      )}
      <span className="node-run-status">
        {data.state}
        {data.httpStatus !== undefined ? ` · HTTP ${data.httpStatus}` : ''}
      </span>
      {data.state === 'active' && <span className="execution-token" aria-label="Active execution" />}
    </div>
  );
}
