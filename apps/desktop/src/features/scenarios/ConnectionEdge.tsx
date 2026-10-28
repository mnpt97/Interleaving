import { BaseEdge, getSmoothStepPath, type EdgeProps } from '@xyflow/react';
import { ConnectionActions } from './ConnectionActions';

export function ConnectionEdge(props: EdgeProps) {
  const [path, x, y] = getSmoothStepPath(props);
  return (
    <>
      <BaseEdge {...props} path={path} labelX={x} labelY={y} />
      <ConnectionActions id={props.id} selected={props.selected} x={x} y={y} />
    </>
  );
}
