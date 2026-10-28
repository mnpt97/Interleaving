import { RunNode, type CapturedNode } from './RunNode';
import { LifecycleRunCanvas } from './LifecycleRunCanvas';
import { useReducedMotion } from './useReducedMotion';
import { IconTooltip } from '../../components/ui/tooltip';
import { RunEdge } from './RunEdge';
import { useMemo, useRef } from 'react';
import {
  Background,
  Controls,
  ControlButton,
  ReactFlow,
  MarkerType,
  type ReactFlowInstance,
  type Edge,
} from '@xyflow/react';
import { ZoomIn, ZoomOut, Maximize } from 'lucide-react';
import { useTheme } from '../../lib/theme';
import { useRunView } from './useRunView';
import { nodeProgress } from './projection';
import '@xyflow/react/dist/style.css';

const nodeTypes = { captured: RunNode };
const edgeTypes = { progress: RunEdge };

/** Render the engine's safe captured catalogue, never a graph reconstructed from the draft. */
export function RunCanvas() {
  const canvasView = useRunView().run.canvasView;
  return canvasView === 'lifecycle' ? <LifecycleRunCanvas /> : <InvocationCanvas />;
}
function InvocationCanvas() {
  const view = useRunView();
  const reduced = useReducedMotion();
  const flow = useRef<ReactFlowInstance<CapturedNode> | null>(null);
  const { resolvedTheme } = useTheme();
  const { run, projection, invocation, workflow } = view;
  const nodes: CapturedNode[] = useMemo(
    () =>
      (workflow?.nodes ?? []).map((node) => {
        const execution = [...(invocation?.nodes ?? [])].reverse().find((value) => value.nodeKey === node.key);
        return {
          id: node.key,
          type: 'captured',
          origin: [0, 0.5],
          position: { ...node.position },
          selected: run.selectedNode === node.key,
          ariaLabel: `${node.name ?? node.id}: ${nodeProgress(invocation, node.key)}`,
          data: { node, state: nodeProgress(invocation, node.key), httpStatus: execution?.httpStatus },
        };
      }),
    [workflow, invocation, run.selectedNode]
  );
  const edges: Edge[] = useMemo(
    () =>
      (workflow?.edges ?? []).map((edge) => {
        const traversed = invocation?.traversed.has(edge.key);
        const skipped = invocation?.skippedEdges.has(edge.key);
        const parallel = workflow!.edges.filter(
          (value) => value.fromNodeKey === edge.fromNodeKey && value.toNodeKey === edge.toNodeKey
        );
        const lane = (parallel.findIndex((value) => value.key === edge.key) - (parallel.length - 1) / 2) * 2;
        return {
          id: edge.key,
          source: edge.fromNodeKey,
          target: edge.toNodeKey,
          sourceHandle: edge.sourceHandle ?? 'right',
          targetHandle: edge.targetHandle ?? 'left',
          type: 'progress',
          data: { lane, motion: !reduced && invocation?.latestEdge === edge.key && invocation?.state === 'active' },
          markerEnd: { type: MarkerType.ArrowClosed },
          className: traversed ? 'run-edge-traversed' : skipped ? 'run-edge-skipped' : '',
          animated: false,
          ariaLabel: `${edge.id}: ${traversed ? 'traversed' : skipped ? 'skipped' : 'unobserved'}`,
        };
      }),
    [workflow, invocation, reduced]
  );
  return (
    <div className="workflow-editor" role="region" aria-label="Run canvas">
      <div className="workflow-stage">
        <ReactFlow
          key={`${run.runId}:${workflow?.key}`}
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          onInit={(instance) => {
            flow.current = instance;
          }}
          colorMode={resolvedTheme}
          fitView
          fitViewOptions={{ padding: 0.3 }}
          minZoom={0.35}
          maxZoom={1.5}
          nodesFocusable
          elementsSelectable
          onNodesChange={(changes) => {
            const selected = changes.find((change) => change.type === 'select' && change.selected);
            if (selected?.type === 'select') run.inspectNode(selected.id, invocation?.key);
          }}
          nodesDraggable={false}
          nodesConnectable={false}
          edgesReconnectable={false}
          deleteKeyCode={null}
          onNodeClick={(_, node) => run.inspectNode(node.id, invocation?.key)}
          onPaneClick={() => run.inspectNode(null)}
          onNodeDoubleClick={(_, node) => {
            const child = projection.invocations.find(
              (value) =>
                value.parentId === invocation?.id &&
                value.callerNodeKey === node.id &&
                value.groupKey === invocation?.groupKey &&
                value.userId === invocation?.userId &&
                value.phase === invocation?.phase &&
                value.iteration === invocation?.iteration
            );
            if (child) run.inspectInvocation(child.key);
          }}
          proOptions={{ hideAttribution: true }}
        >
          <Background gap={10} size={1} color="var(--canvas-grid)" />
          <Controls showInteractive={false} showZoom={false} showFitView={false}>
            <IconTooltip label="Zoom in">
              <ControlButton aria-label="Zoom in" onClick={() => void flow.current?.zoomIn()}>
                <ZoomIn size={16} />
              </ControlButton>
            </IconTooltip>
            <IconTooltip label="Zoom out">
              <ControlButton aria-label="Zoom out" onClick={() => void flow.current?.zoomOut()}>
                <ZoomOut size={16} />
              </ControlButton>
            </IconTooltip>
            <IconTooltip label="Fit workflow to view">
              <ControlButton
                aria-label="Fit workflow to view"
                onClick={() => void flow.current?.fitView({ padding: 0.3 })}
              >
                <Maximize size={16} />
              </ControlButton>
            </IconTooltip>
          </Controls>
        </ReactFlow>
      </div>
    </div>
  );
}
