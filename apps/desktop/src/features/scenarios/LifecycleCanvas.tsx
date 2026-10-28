import { useEffect, useMemo, useRef, useState } from 'react';
import {
  applyNodeChanges,
  Background,
  Controls,
  ControlButton,
  Handle,
  MarkerType,
  Position,
  ReactFlow,
  NodeResizeControl,
  useNodesState,
  type Edge,
  type Node,
  type NodeProps,
  type NodeTypes,
  type ReactFlowInstance,
} from '@xyflow/react';
import {
  Play,
  FlagTriangleRight,
  ExternalLink,
  Copy,
  Maximize,
  ZoomIn,
  ZoomOut,
  MoveDiagonal2,
  GripVertical,
} from 'lucide-react';
import { Button } from '../../components/ui/button';
import { IconTooltip } from '../../components/ui/tooltip';
import { AddNodePopover } from './AddNodePopover';
import { ScenarioNode } from './ScenarioNode';
import { toFlowEdges } from './adapters';
import { ConnectionEdge } from './ConnectionEdge';
import { RouteEdge } from './RouteEdge';
import { connectionError } from './graph';
import { phaseBoxes, phaseWorkflow, phaseName, scopedId, localId, type PhaseBox, type Phase } from './lifecycle';
import { useScenarioStore } from './store';
import { useTheme } from '../../lib/theme';
import { InitialCanvasFit } from './InitialCanvasFit';
import { usePhaseLayout } from './usePhaseLayout';

function PhaseContainer({ data }: NodeProps) {
  const box = data.box as PhaseBox;
  const state = useScenarioStore();
  return (
    <div
      className={`lifecycle-phase ${state.selectedPhase === box.phase ? 'is-active' : ''}`}
      style={{ width: box.width, height: box.height }}
    >
      {state.mode === 'edit' && !state.pendingChange && !state.busy && (
        <NodeResizeControl
          position="bottom-right"
          minWidth={320}
          minHeight={180}
          className="phase-resize-control"
          onResize={(_, size) => (data.resize as (width: number, height: number) => void)(size.width, size.height)}
        >
          <MoveDiagonal2 aria-label={`Resize ${phaseName[box.phase]}`} size={16} />
        </NodeResizeControl>
      )}
      <Handle type="target" position={Position.Top} isConnectable={false} style={{ visibility: 'hidden' }} />
      <Handle type="source" position={Position.Bottom} isConnectable={false} style={{ visibility: 'hidden' }} />
      <div className="phase-heading nopan">
        <GripVertical size={16} className="phase-drag-grip" aria-label={`Move ${phaseName[box.phase]}`} />
        <button className="nodrag" onClick={() => state.selectPhase(box.phase)}>
          {phaseName[box.phase]}
        </button>
        <div className="phase-actions nodrag">
          <Button
            size="icon"
            variant="ghost"
            aria-label={`Fit ${phaseName[box.phase]}`}
            onClick={() => (data.fit as () => void)()}
          >
            <Maximize size={14} />
          </Button>
          {state.mode === 'edit' && box.inline && <AddNodePopover phase={box.phase} />}
          {state.mode === 'edit' && !box.inline && (
            <Button
              size="icon"
              variant="ghost"
              aria-label={`Detach ${phaseName[box.phase]} copy`}
              onClick={() => state.command({ type: 'phase.detach', phase: box.phase })}
            >
              <Copy size={14} />
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
export function PhasePort({ data }: NodeProps) {
  const mode = useScenarioStore((state) => state.mode);
  const start = data.start as boolean;
  return (
    <div className="phase-port" title={`${data.phase} ${start ? 'entry' : 'exit'}`}>
      {(
        [
          ['left', Position.Left],
          ['right', Position.Right],
          ['top', Position.Top],
          ['bottom', Position.Bottom],
        ] as const
      ).map(([id, position]) => (
        <Handle
          key={id}
          id={id}
          position={position}
          type={start ? 'source' : 'target'}
          isConnectable={mode === 'edit'}
          style={{ visibility: mode === 'edit' ? 'visible' : 'hidden' }}
        />
      ))}
    </div>
  );
}
function GlobalBoundary({ data }: NodeProps) {
  return (
    <div className={`scenario-node scenario-node-boundary scenario-node-${data.start ? 'start' : 'end'}`}>
      {data.start ? <Play aria-label="Scenario start" /> : <FlagTriangleRight aria-label="Scenario end" />}
      <Handle
        type={data.start ? 'source' : 'target'}
        position={data.start ? Position.Bottom : Position.Top}
        isConnectable={false}
        style={{ visibility: 'hidden' }}
      />
    </div>
  );
}
function Reference({ data }: NodeProps) {
  const box = data.box as PhaseBox;
  const state = useScenarioStore();
  return (
    <div className="scenario-node phase-reference">
      <strong>{box.workflow?.name}</strong>
      <Button size="sm" variant="ghost" onClick={() => box.workflow && state.selectWorkflow(box.workflow.id)}>
        <ExternalLink size={14} />
        Open workflow
      </Button>
      <Handle type="target" position={Position.Left} isConnectable={false} style={{ visibility: 'hidden' }} />
      <Handle type="source" position={Position.Right} isConnectable={false} style={{ visibility: 'hidden' }} />
    </div>
  );
}
const nodeTypes: NodeTypes = {
  scenario: ScenarioNode,
  phase: PhaseContainer,
  phasePort: PhasePort,
  globalBoundary: GlobalBoundary,
  phaseReference: Reference,
};
const edgeTypes = { smoothstep: ConnectionEdge, exclusiveRoute: RouteEdge };

/** Snap node centers, with a small alignment magnet on the phase's entry and exit rows. */
export function snapPhasePosition(box: PhaseBox, position: { x: number; y: number }) {
  const rows =
    box.workflow?.nodes
      .filter((node) => node.type === 'start' || node.type === 'end')
      .map((node) =>
        Math.max(120, Math.min(box.height - 40, (box.workflow?.layout?.positions[node.id]?.y ?? 160) - box.minY + 40))
      ) ?? [];
  const aligned = rows.find((y) => Math.abs(y - position.y) <= 10);
  return { x: Math.round(position.x / 10) * 10, y: aligned ?? Math.round(position.y / 10) * 10 };
}

/** Layout is a projection; stored positions remain workflow-local and membership remains binding-owned. */
export function lifecycleNodes(boxes: PhaseBox[], selectedPhase: Phase, selectedId: string | null): Node[] {
  if (!boxes.length) return [];
  const nodes: Node[] = [];
  for (const box of boxes) {
    const parentId = `phase:${box.phase}`;
    nodes.push({
      id: parentId,
      type: 'phase',
      position: { x: box.x, y: box.y },
      data: { box },
      style: { width: box.width, height: box.height },
      draggable: true,
      dragHandle: '.phase-heading',
      selectable: false,
      connectable: false,
    });
    if (box.inline) {
      for (const [index, node] of (box.workflow?.nodes ?? []).entries()) {
        const boundary = node.type === 'start' || node.type === 'end';
        const position = box.workflow?.layout?.positions[node.id] ?? { x: 80 + index * 260, y: 160 };
        nodes.push({
          id: scopedId(box.phase, node.id),
          parentId,
          type: boundary ? 'phasePort' : 'scenario',
          origin: boundary ? [0.5, 0.5] : [0, 0.5],
          position: boundary
            ? {
                x: node.type === 'start' ? 32 : box.width - 32,
                y: Math.max(120, Math.min(box.height - 40, position.y - box.minY + 40)),
              }
            : { x: position.x - box.minX + 80, y: position.y - box.minY + 40 },
          data: { node, phase: box.phase, workflowId: box.workflow!.id, start: node.type === 'start' },
          draggable: !boundary,
          selected: selectedPhase === box.phase && selectedId === node.id,
          ariaLabel: boundary
            ? `${phaseName[box.phase]} ${node.type === 'start' ? 'entry' : 'exit'}`
            : `${phaseName[box.phase]}: ${node.name || node.id}`,
        });
      }
    } else
      nodes.push({
        id: scopedId(box.phase, '@reference'),
        type: 'phaseReference',
        parentId,
        position: { x: 80, y: 100 },
        data: { box },
        draggable: false,
        connectable: false,
      });
  }
  nodes.push(
    {
      id: 'scenario:start',
      type: 'globalBoundary',
      origin: [0.5, 0.5],
      position: { x: boxes[0].x + boxes[0].width / 2, y: 40 },
      data: { start: true },
      draggable: false,
      selectable: false,
      connectable: false,
    },
    {
      id: 'scenario:end',
      type: 'globalBoundary',
      origin: [0.5, 0.5],
      position: {
        x: boxes[0].x + boxes[0].width / 2,
        y: boxes[boxes.length - 1].y + boxes[boxes.length - 1].height + 100,
      },
      data: { start: false },
      draggable: false,
      selectable: false,
      connectable: false,
    }
  );
  return nodes;
}
export function LifecycleCanvas() {
  const state = useScenarioStore();
  const { resolvedTheme } = useTheme();
  const document = state.document;
  const defaults = useMemo(() => (document ? phaseBoxes(document) : []), [document]);
  const { boxes, resize, move: movePhase } = usePhaseLayout(document?.id ?? '', defaults);
  const flow = useRef<ReactFlowInstance | null>(null);
  const [selectedEdge, setSelectedEdge] = useState<string | null>(null);
  const reconnecting = useRef<string | undefined>(undefined);
  useEffect(() => setSelectedEdge(null), [state.selectedPhase, state.mode, document?.id]);
  const [nodes, setNodes] = useNodesState<Node>([]);
  const editable = state.mode === 'edit' && !state.pendingChange && !state.busy;
  useEffect(() => {
    setNodes((current) =>
      lifecycleNodes(boxes, state.selectedPhase, state.selectedId).map((node) => ({
        ...current.find((old) => old.id === node.id),
        ...node,
        draggable: editable && Boolean(node.draggable),
        data: {
          ...node.data,
          fit: () => void flow.current?.fitView({ nodes: [{ id: node.id }], padding: 0.2 }),
          resize: (width: number, height: number) => {
            const box = node.data.box as PhaseBox | undefined;
            if (box) resize(box.phase, width, height);
          },
        },
      }))
    );
  }, [boxes, state.selectedPhase, state.selectedId, setNodes, resize, editable]);
  const edges = useMemo<Edge[]>(() => {
    if (!document) return [];
    const result: Edge[] = boxes.flatMap((box) =>
      box.inline && box.workflow
        ? toFlowEdges(document, box.workflow.id).map((edge) => ({
            ...edge,
            id: scopedId(box.phase, edge.id),
            source: scopedId(box.phase, edge.source),
            target: scopedId(box.phase, edge.target),
            selected: selectedEdge === scopedId(box.phase, edge.id),
          }))
        : []
    );
    // Phase-level connectors indicate order only; the lifecycle controller owns execution.
    const sequence = ['scenario:start', ...boxes.map((box) => `phase:${box.phase}`), 'scenario:end'];
    sequence.slice(1).forEach((target, index) =>
      result.push({
        id: `lifecycle:${index}`,
        source: sequence[index],
        target,
        selectable: false,
        reconnectable: false,
        type: 'smoothstep',
        className: 'lifecycle-order-edge',
        markerEnd: { type: MarkerType.ArrowClosed },
      })
    );
    return result;
  }, [document, boxes, selectedEdge]);
  const connection = (source: string | null, target: string | null, replacing?: string) => {
    const from = source && localId(source),
      to = target && localId(target);
    if (!editable || !document || !from || !to || from.phase !== to.phase) return null;
    const workflow = phaseWorkflow(document, from.phase);
    if (!workflow || connectionError(workflow, from.id, to.id, replacing && localId(replacing)?.id)) return null;
    return { workflow, from, to };
  };
  if (!document) return null;
  const remove = () => {
    const edge = selectedEdge && localId(selectedEdge);
    const phase = edge ? edge.phase : state.selectedPhase;
    const workflow = phaseWorkflow(document, phase);
    if (!editable || !workflow) return;
    state.command({
      type: 'graph.delete',
      workflowId: workflow.id,
      nodeIds: edge ? [] : state.selectedId ? [state.selectedId] : [],
      edgeIds: edge ? [edge.id] : [],
    });
    if (!useScenarioStore.getState().error) setSelectedEdge(null);
  };
  return (
    <div
      className="workflow-editor"
      role="region"
      aria-label="Scenario lifecycle"
      tabIndex={0}
      onKeyDown={(event) => {
        if (
          (event.target as HTMLElement).closest('input,textarea,select,button,[contenteditable="true"],[role="dialog"]')
        )
          return;
        if (editable && ['Delete', 'Backspace'].includes(event.key)) {
          event.preventDefault();
          remove();
        }
      }}
    >
      <div className="workflow-stage">
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          onInit={(instance) => {
            flow.current = instance;
          }}
          colorMode={resolvedTheme}
          fitView
          fitViewOptions={{ padding: 0.12 }}
          minZoom={0.01}
          maxZoom={1.5}
          nodesDraggable={editable}
          nodesConnectable={editable}
          edgesReconnectable={editable}
          snapToGrid={false}
          snapGrid={[10, 10]}
          deleteKeyCode={null}
          proOptions={{ hideAttribution: true }}
          onNodeClick={(_, node) => {
            const local = localId(node.id);
            if (local && local.id !== '@reference') {
              setSelectedEdge(null);
              const box = boxes.find((box) => box.phase === local.phase);
              const source = box?.workflow?.nodes.find((node) => node.id === local.id);
              state.selectPhase(
                local.phase,
                source && source.type !== 'start' && source.type !== 'end' ? local.id : null
              );
            }
          }}
          onNodesChange={(changes) => {
            // React Flow snaps top-left bounds. Snap our center-origin coordinates instead.
            const snapped = changes.map((change) => {
              if (change.type !== 'position' || !change.position) return change;
              const local = localId(change.id);
              const box = local && boxes.find((box) => box.phase === local.phase);
              return {
                ...change,
                position: box
                  ? snapPhasePosition(box, change.position)
                  : {
                      x: Math.round(change.position.x / 10) * 10,
                      y: Math.round(change.position.y / 10) * 10,
                    },
              };
            });
            setNodes((current) =>
              applyNodeChanges(
                snapped.filter(
                  (change) =>
                    change.type === 'dimensions' || change.type === 'select' || (editable && change.type === 'position')
                ),
                current
              )
            );
            for (const change of snapped) {
              if (!('id' in change)) continue;
              const local = localId(change.id);
              if (!local) continue;
              const box = boxes.find((box) => box.phase === local.phase);
              if (
                editable &&
                change.type === 'position' &&
                change.position &&
                box?.workflow &&
                change.dragging === undefined
              ) {
                const x = Math.max(80, change.position.x),
                  y = Math.max(120, change.position.y);
                state.command(
                  {
                    type: 'node.move',
                    workflowId: box.workflow.id,
                    nodeId: local.id,
                    position: { x: x + box.minX - 80, y: y + box.minY - 40 },
                  },
                  `move:${box.workflow.id}:${local.id}`
                );
              }
              if (change.type === 'select' && change.selected) state.selectPhase(local.phase, local.id);
            }
          }}
          onNodeDragStop={(_, node) => {
            const frame = boxes.find((box) => node.id === `phase:${box.phase}`);
            if (editable && frame) {
              movePhase(frame.phase, node.position);
              return;
            }
            const local = localId(node.id);
            const source = local && boxes.find((box) => box.phase === local.phase);
            if (!editable || !local || !source?.workflow) return;
            const position = snapPhasePosition(source, node.position);
            const absolute = { x: source.x + position.x, y: source.y + position.y };
            const target = boxes.find(
              (box) =>
                box.phase !== local.phase &&
                absolute.x >= box.x &&
                absolute.x <= box.x + box.width &&
                absolute.y >= box.y &&
                absolute.y <= box.y + box.height
            );
            if (target)
              state.command({
                type: 'phase.move',
                from: local.phase,
                to: target.phase,
                nodeIds: [local.id],
                position: {
                  x: Math.max(80, Math.min(target.width - 260, absolute.x - target.x)) + target.minX - 80,
                  y: Math.max(120, Math.min(target.height - 60, absolute.y - target.y)) + target.minY - 40,
                },
              });
            else
              state.command({
                type: 'node.move',
                workflowId: source.workflow.id,
                nodeId: local.id,
                position: {
                  x: Math.max(80, Math.min(source.width - 260, position.x)) + source.minX - 80,
                  y: Math.max(120, Math.min(source.height - 60, position.y)) + source.minY - 40,
                },
              });
            // Rebuild on rejected transfers as well, returning the node to its source phase.
            setNodes(
              lifecycleNodes(
                boxes,
                useScenarioStore.getState().selectedPhase,
                useScenarioStore.getState().selectedId
              ).map((value) => ({
                ...value,
                data: {
                  ...value.data,
                  fit: () => void flow.current?.fitView({ nodes: [{ id: value.id }], padding: 0.2 }),
                  resize: (width: number, height: number) => {
                    const box = value.data.box as PhaseBox | undefined;
                    if (box) resize(box.phase, width, height);
                  },
                },
              }))
            );
            state.endHistoryGroup();
          }}
          isValidConnection={(value) => Boolean(connection(value.source, value.target, reconnecting.current))}
          onReconnectStart={(_, edge) => {
            reconnecting.current = edge.id;
          }}
          onReconnectEnd={() => {
            reconnecting.current = undefined;
          }}
          onConnect={(value) => {
            const valid = connection(value.source, value.target);
            if (valid)
              state.command({
                type: 'edge.connect',
                workflowId: valid.workflow.id,
                from: valid.from.id,
                to: valid.to.id,
                sourceHandle: (value.sourceHandle ?? 'right') as 'left' | 'right' | 'top' | 'bottom',
                targetHandle: (value.targetHandle ?? 'left') as 'left' | 'right' | 'top' | 'bottom',
              });
          }}
          onReconnect={(edge, value) => {
            const valid = connection(value.source, value.target, edge.id);
            const local = localId(edge.id);
            if (valid && local && local.phase === valid.from.phase)
              state.command({
                type: 'edge.reconnect',
                workflowId: valid.workflow.id,
                edgeId: local.id,
                from: valid.from.id,
                to: valid.to.id,
                sourceHandle: (value.sourceHandle ?? 'right') as 'left' | 'right' | 'top' | 'bottom',
                targetHandle: (value.targetHandle ?? 'left') as 'left' | 'right' | 'top' | 'bottom',
              });
          }}
          onEdgeClick={(_, edge) => {
            const local = localId(edge.id);
            if (local) {
              state.selectPhase(local.phase);
              if (useScenarioStore.getState().selectedPhase === local.phase) setSelectedEdge(edge.id);
            }
          }}
          onPaneClick={() => {
            setSelectedEdge(null);
            state.select(null);
          }}
        >
          <InitialCanvasFit sceneKey={`${state.entryKey}:${document.id}`} />
          <Background gap={10} size={1} color="var(--canvas-grid)" />
          <Controls showInteractive={false} showZoom={false} showFitView={false}>
            {[
              ['Zoom in', ZoomIn, () => flow.current?.zoomIn()],
              ['Zoom out', ZoomOut, () => flow.current?.zoomOut()],
              ['Fit scenario', Maximize, () => flow.current?.fitView({ padding: 0.12 })],
            ].map(([label, Icon, action]) => {
              const Graphic = Icon as typeof ZoomIn;
              return (
                <IconTooltip key={String(label)} label={String(label)}>
                  <ControlButton aria-label={String(label)} onClick={() => void (action as () => unknown)()}>
                    <Graphic size={16} />
                  </ControlButton>
                </IconTooltip>
              );
            })}
          </Controls>
        </ReactFlow>
      </div>
    </div>
  );
}
