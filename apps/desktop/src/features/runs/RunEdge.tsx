import { BaseEdge, getSmoothStepPath, type Edge, type EdgeProps } from '@xyflow/react';

type ProgressEdge = Edge<{ lane: number; motion: boolean }>;
/** Animate at most the last observed traversal; static highlights retain its meaning. */
export function RunEdge(props: EdgeProps<ProgressEdge>) {
  let [path] = getSmoothStepPath(props);
  const lane = props.data?.lane ?? 0;
  if (lane) {
    const directions = { left: [-1, 0], right: [1, 0], top: [0, -1], bottom: [0, 1] };
    const [sx, sy] = directions[props.sourcePosition];
    const [tx, ty] = directions[props.targetPosition];
    const bend = Math.max(40, Math.hypot(props.targetX - props.sourceX, props.targetY - props.sourceY) / 3);
    const dx = sy ? lane * 64 : 0;
    const dy = sx ? lane * 64 : 0;
    path = `M ${props.sourceX},${props.sourceY} C ${props.sourceX + sx * bend + dx},${props.sourceY + sy * bend + dy} ${props.targetX + tx * bend + dx},${props.targetY + ty * bend + dy} ${props.targetX},${props.targetY}`;
  }
  return (
    <>
      <BaseEdge {...props} path={path} />
      {props.data?.motion && (
        <circle className="run-edge-token" r={4} fill="var(--primary)" pointerEvents="none" aria-hidden="true">
          <animateMotion path={path} dur="0.25s" repeatCount={1} fill="freeze" />
        </circle>
      )}
    </>
  );
}
