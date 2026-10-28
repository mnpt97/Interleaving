import { useMemo, useRef } from 'react';
import {
  Background,
  Controls,
  ControlButton,
  Handle,
  MarkerType,
  Position,
  ReactFlow,
  type Edge,
  type Node,
  type NodeProps,
  type NodeTypes,
  type ReactFlowInstance,
} from '@xyflow/react';
import { ExternalLink, Maximize, ZoomIn, ZoomOut } from 'lucide-react';
import { Button } from '../../components/ui/button';
import { IconTooltip } from '../../components/ui/tooltip';
import { phaseName, phases, scopedId, phaseContentBounds, type Phase } from '../scenarios/lifecycle';
import { nodeProgress, type Invocation, type Progress, type Projection } from './projection';
import { RunNode } from './RunNode';
import { RunEdge } from './RunEdge';
import { useRunView } from './useRunView';
import { useReducedMotion } from './useReducedMotion';
import { useRunStore } from './store';
import { useTheme } from '../../lib/theme';
import type { RunReport } from '../../lib/run-types';
import { InitialCanvasFit } from '../scenarios/InitialCanvasFit';

function Frame({ data }: NodeProps) {
  const phase = data.phase as Phase;
  const root = data.root as Invocation | undefined;
  const run = useRunStore();
  return (
    <div className="lifecycle-phase" style={{ width: data.width as number, height: data.height as number }}>
      <div className="phase-heading">
        <button onClick={() => root && run.inspectInvocation(root.key)}>{phaseName[phase]}</button>
        <span className={`phase-outcome state-${data.state}`}>{String(data.state)}</span>
      </div>
      <Handle
        id="bottom"
        type="source"
        position={Position.Bottom}
        isConnectable={false}
        style={{ visibility: 'hidden' }}
      />
      <Handle id="top" type="target" position={Position.Top} isConnectable={false} style={{ visibility: 'hidden' }} />
    </div>
  );
}
function Reference({ data }: NodeProps) {
  const run = useRunStore();
  const root = data.root as Invocation | undefined;
  return (
    <div className="scenario-node phase-reference">
      <strong>{String(data.name)}</strong>
      <Button size="sm" variant="ghost" disabled={!root} onClick={() => root && run.openInvocation(root.key)}>
        <ExternalLink size={14} />
        Open workflow
      </Button>
    </div>
  );
}
const nodeTypes: NodeTypes = { captured: RunNode, runPhase: Frame, runPhaseReference: Reference };
const edgeTypes = { progress: RunEdge };
const outcome = (status: RunReport['status']): Progress =>
  status === 'passed' ? 'completed' : status === 'cancelled' ? 'cancelled' : 'failed';

/** Phase connectors are presentation only. Only engine edge observations receive progress. */
export function lifecycleRunModel(
  projection: Projection,
  report: RunReport | null,
  reduced: boolean,
  selectedNode: string | null,
  selectedInvocation: string | null
) {
  const group = projection.graph?.groups[0];
  const nodes: Node[] = [];
  const edges: Edge[] = [];
  if (!group) return { nodes, edges };
  let y = 140;
  for (const phase of phases) {
    const workflow = projection.graph?.workflows.find((workflow) => workflow.key === group[phase]);
    const root = [...projection.invocations]
      .reverse()
      .find((value) => !value.parentId && value.groupKey === group.key && value.phase === phase);
    const inline = Boolean(group.inlinePhases?.includes(phase));
    const positions = inline
      ? (workflow?.nodes.filter((node) => node.type !== 'start' && node.type !== 'end').map((node) => node.position) ??
        [])
      : [];
    const { minX, minY, width, height } = phaseContentBounds(positions, !inline && Boolean(workflow));
    const state = projection.phases[phase] ?? (workflow ? (report ? 'unknown' : 'pending') : 'not configured');
    const parentId = `run-phase:${phase}`;
    nodes.push({
      id: parentId,
      type: 'runPhase',
      position: { x: 80, y },
      data: { phase, root, width, height, state },
      style: { width, height },
      draggable: false,
      connectable: false,
      selectable: false,
    });
    if (inline && workflow) {
      for (const node of workflow.nodes) {
        const port = node.type === 'start' || node.type === 'end';
        const execution = [...(root?.nodes ?? [])].reverse().find((value) => value.nodeKey === node.key);
        const progress = root ? nodeProgress(root, node.key) : state === 'pending' ? 'pending' : 'unknown';
        nodes.push({
          id: scopedId(phase, node.key),
          type: 'captured',
          parentId,
          origin: port ? [0.5, 0.5] : [0, 0.5],
          position: port
            ? {
                x: node.type === 'start' ? 32 : width - 32,
                y: Math.max(120, Math.min(height - 40, node.position.y - minY + 40)),
              }
            : { x: node.position.x - minX + 80, y: node.position.y - minY + 40 },
          selected: selectedNode === node.key && selectedInvocation === root?.key,
          draggable: false,
          connectable: false,
          ariaLabel: `${phaseName[phase]}: ${node.name ?? node.id} · ${progress}`,
          data: {
            node,
            state: progress,
            httpStatus: execution?.httpStatus,
            port,
            phase,
            wireKey: node.key,
            invocationKey: root?.key,
          },
        });
      }
      for (const edge of workflow.edges) {
        const parallel = workflow.edges.filter(
          (other) => other.fromNodeKey === edge.fromNodeKey && other.toNodeKey === edge.toNodeKey
        );
        const lane = (parallel.findIndex((other) => other.key === edge.key) - (parallel.length - 1) / 2) * 2;
        const traversed = root?.traversed.has(edge.key),
          skipped = root?.skippedEdges.has(edge.key);
        edges.push({
          id: scopedId(phase, edge.key),
          source: scopedId(phase, edge.fromNodeKey),
          target: scopedId(phase, edge.toNodeKey),
          sourceHandle: edge.sourceHandle ?? 'right',
          targetHandle: edge.targetHandle ?? 'left',
          type: 'progress',
          data: { lane, motion: !reduced && root?.latestEdge === edge.key && root?.state === 'active' },
          markerEnd: { type: MarkerType.ArrowClosed },
          className: traversed ? 'run-edge-traversed' : skipped ? 'run-edge-skipped' : '',
          ariaLabel: `${phaseName[phase]}: ${edge.id} · ${traversed ? 'traversed' : skipped ? 'skipped' : 'unobserved'}`,
        });
      }
    } else if (workflow)
      nodes.push({
        id: `reference:${phase}`,
        type: 'runPhaseReference',
        parentId,
        position: { x: 80, y: 100 },
        data: { name: workflow.name, root },
        draggable: false,
        connectable: false,
      });
    y += height + 100;
  }
  const frames = nodes.filter((node) => node.type === 'runPhase');
  const widest = Math.max(...frames.map((frame) => frame.data.width as number));
  for (const frame of frames) frame.position.x = 80 + (widest - (frame.data.width as number)) / 2;
  const x = 80 + widest / 2;
  const final = projection.otherEvents.find((event) => event.type === 'run.finished');
  const status = report?.status ?? final?.payload.status;
  nodes.push(
    {
      id: 'run:start',
      type: 'captured',
      position: { x, y: 40 },
      origin: [0.5, 0.5],
      data: {
        node: { key: 'run:start', id: 'start', type: 'start', position: { x, y: 40 } },
        state: projection.otherEvents.some((event) => event.type === 'run.started') ? 'completed' : 'unknown',
      },
      selectable: false,
      draggable: false,
      connectable: false,
    },
    {
      id: 'run:end',
      type: 'captured',
      position: { x, y },
      origin: [0.5, 0.5],
      data: {
        node: { key: 'run:end', id: 'end', type: 'end', position: { x, y } },
        state: status ? outcome(status) : 'pending',
      },
      selectable: false,
      draggable: false,
      connectable: false,
    }
  );
  const sequence = ['run:start', ...phases.map((phase) => `run-phase:${phase}`), 'run:end'];
  sequence.slice(1).forEach((target, index) =>
    edges.push({
      id: `run-order:${index}`,
      source: sequence[index],
      target,
      sourceHandle: 'bottom',
      targetHandle: 'top',
      selectable: false,
      type: 'progress',
      data: { lane: 0, motion: false },
      className: 'lifecycle-order-edge',
      markerEnd: { type: MarkerType.ArrowClosed },
    })
  );
  return { nodes, edges };
}
export function LifecycleRunCanvas() {
  const { run, projection } = useRunView();
  const reduced = useReducedMotion();
  const { resolvedTheme } = useTheme();
  const flow = useRef<ReactFlowInstance | null>(null);
  const model = useMemo(
    () => lifecycleRunModel(projection, run.report, reduced, run.selectedNode, run.selectedInvocation),
    [projection, run.report, reduced, run.selectedNode, run.selectedInvocation]
  );
  return (
    <div className="workflow-editor" role="region" aria-label="Run lifecycle canvas">
      <div className="workflow-stage">
        <ReactFlow
          nodes={model.nodes}
          edges={model.edges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          colorMode={resolvedTheme}
          onInit={(instance) => {
            flow.current = instance;
          }}
          fitView
          fitViewOptions={{ padding: 0.12 }}
          minZoom={0.01}
          maxZoom={1.5}
          nodesDraggable={false}
          nodesConnectable={false}
          edgesReconnectable={false}
          deleteKeyCode={null}
          proOptions={{ hideAttribution: true }}
          onNodeClick={(_, node) => {
            if (typeof node.data.wireKey === 'string')
              run.inspectNode(
                node.data.wireKey,
                typeof node.data.invocationKey === 'string' ? node.data.invocationKey : undefined
              );
          }}
          onPaneClick={() => run.inspectNode(null)}
          onNodeDoubleClick={(_, node) => {
            const root = projection.invocations.find((value) => value.key === node.data.invocationKey);
            const child = projection.invocations.find(
              (value) =>
                value.parentId === root?.id &&
                value.callerNodeKey === node.data.wireKey &&
                value.groupKey === root?.groupKey &&
                value.userId === root?.userId &&
                value.phase === root?.phase &&
                value.iteration === root?.iteration
            );
            if (child) run.openInvocation(child.key);
          }}
        >
          <InitialCanvasFit sceneKey={run.runId ?? 'run'} />
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
            <IconTooltip label="Fit scenario">
              <ControlButton aria-label="Fit scenario" onClick={() => void flow.current?.fitView({ padding: 0.12 })}>
                <Maximize size={16} />
              </ControlButton>
            </IconTooltip>
          </Controls>
        </ReactFlow>
      </div>
    </div>
  );
}
