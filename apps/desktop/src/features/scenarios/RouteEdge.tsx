import { BaseEdge, type Edge, type EdgeProps } from '@xyflow/react';
import { ConnectionActions } from './ConnectionActions';

/** Separate parallel drawing paths while respecting each selected attachment side. */
export function RouteEdge({
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  data,
  ...props
}: EdgeProps<Edge<{ lane: number }>>) {
  const directions = { left: [-1, 0], right: [1, 0], top: [0, -1], bottom: [0, 1] };
  const [sx, sy] = directions[sourcePosition];
  const [tx, ty] = directions[targetPosition];
  const lift = (data?.lane ?? 1) * 64;
  const bend = Math.max(40, Math.hypot(targetX - sourceX, targetY - sourceY) / 3);
  const dx = sy ? lift : 0;
  const dy = sx ? lift : 0;
  const path = `M ${sourceX},${sourceY} C ${sourceX + sx * bend + dx},${sourceY + sy * bend + dy} ${targetX + tx * bend + dx},${targetY + ty * bend + dy} ${targetX},${targetY}`;
  return (
    <>
      <BaseEdge
        {...props}
        path={path}
        labelX={(sourceX + targetX) / 2 + dx * 0.75}
        labelY={(sourceY + targetY) / 2 + dy * 0.75}
      />
      <ConnectionActions
        id={props.id}
        selected={props.selected}
        x={(sourceX + targetX) / 2 + dx * 0.75}
        y={(sourceY + targetY) / 2 + dy * 0.75}
      />
    </>
  );
}
