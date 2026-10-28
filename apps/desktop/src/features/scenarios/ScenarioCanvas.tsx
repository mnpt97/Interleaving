import { ScenarioNode } from './ScenarioNode';
import { LifecycleCanvas } from './LifecycleCanvas';
import { RunCanvas } from '../runs/RunCanvas';
import { useRunView } from '../runs/useRunView';
import {
  applyNodeChanges,
  Background,
  Controls,
  ConnectionMode,
  ControlButton,
  type ReactFlowInstance,
  ReactFlow,
  useNodesState,
  type NodeTypes,
} from '@xyflow/react';
import { selectedWorkflow } from '../../lib/scenario-types';
import { useEffect, useMemo, useRef, useState } from 'react';
import { ArrowLeft, ZoomIn, ZoomOut, Maximize } from 'lucide-react';
import { Button } from '../../components/ui/button';
import { IconTooltip } from '../../components/ui/tooltip';
import { RouteEdge } from './RouteEdge';
import { connectionError } from './graph';
import { ConnectionEdge } from './ConnectionEdge';
import { toFlowEdges, toFlowNodes, type ScenarioFlowNode } from './adapters';
import { useScenarioStore } from './store';
import '@xyflow/react/dist/style.css';
import { useTheme } from '../../lib/theme';

const nodeTypes: NodeTypes = { scenario: ScenarioNode };
const edgeTypes = { exclusiveRoute: RouteEdge, smoothstep: ConnectionEdge };

export function ScenarioCanvas() {
  const view = useRunView();
  const canvasView = useScenarioStore((state) => state.canvasView);
  if (view.viewing) return <RunCanvas />;
  return canvasView === 'lifecycle' ? <LifecycleCanvas /> : <DraftCanvas />;
}
function DraftCanvas() {
  const { resolvedTheme } = useTheme();
  const draft = useScenarioStore((s) => s.document);
  const mode = useScenarioStore((s) => s.mode);
  const document = draft;
  const workflowId = useScenarioStore((s) => s.selectedWorkflowId);
  const selected = useScenarioStore((s) => s.selectedId);
  const select = useScenarioStore((s) => s.select);
  const move = useScenarioStore((s) => s.move);
  // Measurements and selection are transient React Flow state, never domain data.
  const [nodes, setNodes] = useNodesState<ScenarioFlowNode>(
    document ? toFlowNodes(document, selected, workflowId) : []
  );
  useEffect(() => {
    setNodes((current) =>
      document
        ? toFlowNodes(document, selected, workflowId).map((node) => ({
            ...current.find((previous) => previous.id === node.id),
            ...node,
          }))
        : []
    );
  }, [document, selected, workflowId, setNodes]);
  const [selectedEdge, setSelectedEdge] = useState<string | null>(null);
  const flow = useRef<Pick<ReactFlowInstance<ScenarioFlowNode>, 'zoomIn' | 'zoomOut' | 'fitView'> | null>(null);
  const reconnecting = useRef<string | undefined>(undefined);
  const state = useScenarioStore();
  const workflow = document ? selectedWorkflow(document, workflowId) : undefined;
  useEffect(() => setSelectedEdge(null), [workflowId, mode]);
  const edges = useMemo(
    () =>
      document
        ? toFlowEdges(document, workflowId).map((edge) => ({ ...edge, selected: edge.id === selectedEdge }))
        : [],
    [document, workflowId, selectedEdge]
  );
  const editable = mode === 'edit' && !state.pendingChange && !state.busy;
  const deleteSelection = () => {
    if (!editable || !workflow) return;
    state.command({
      type: 'graph.delete',
      workflowId: workflow.id,
      nodeIds: selectedEdge ? [] : selected ? [selected] : [],
      edgeIds: selectedEdge ? [selectedEdge] : [],
    });
    if (!useScenarioStore.getState().error) setSelectedEdge(null);
  };
  if (!document || !workflow)
    return (
      <div className="canvas-empty">
        <h2>Unable to render this scenario</h2>
        <p>Review the diagnostics or start a new scenario to create an iteration workflow.</p>
      </div>
    );
  return (
    <div
      className="workflow-editor"
      role="region"
      aria-label="Workflow editor"
      tabIndex={0}
      onKeyDown={(event) => {
        const target = event.target as HTMLElement;
        if (target.closest('input, textarea, select, button, [contenteditable="true"], [role="dialog"]')) return;
        if ((event.key === 'Delete' || event.key === 'Backspace') && editable && (selected || selectedEdge)) {
          event.preventDefault();
          event.stopPropagation();
          deleteSelection();
        }
      }}
    >
      {(state.workflowTrail.length > 0 || state.canvasView === 'workflow') && (
        <div className="canvas-context-bar">
          <Button
            size="sm"
            variant="ghost"
            onClick={state.workflowTrail.length ? state.leaveWorkflow : state.showLifecycle}
          >
            <ArrowLeft />
            {state.workflowTrail.length ? 'Back to caller' : 'Back to scenario'}
          </Button>
        </div>
      )}
      <div className="workflow-stage">
        <ReactFlow
          proOptions={{ hideAttribution: true }}
          onInit={(instance) => {
            flow.current = instance;
          }}
          colorMode={resolvedTheme}
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          fitView
          fitViewOptions={{ padding: 0.3 }}
          minZoom={0.35}
          maxZoom={1.5}
          connectionMode={ConnectionMode.Strict}
          connectionRadius={28}
          nodesConnectable={editable}
          edgesReconnectable={editable}
          onReconnectStart={(_, edge) => {
            reconnecting.current = edge.id;
          }}
          onReconnectEnd={() => {
            reconnecting.current = undefined;
          }}
          isValidConnection={(connection) =>
            Boolean(
              editable &&
              workflow &&
              !connectionError(workflow, connection.source, connection.target, reconnecting.current)
            )
          }
          onConnect={(connection) => {
            if (editable && workflow)
              state.command({
                type: 'edge.connect',
                workflowId: workflow.id,
                from: connection.source,
                to: connection.target,
                sourceHandle: (connection.sourceHandle ?? 'right') as 'left' | 'right' | 'top' | 'bottom',
                targetHandle: (connection.targetHandle ?? 'left') as 'left' | 'right' | 'top' | 'bottom',
              });
          }}
          onReconnect={(edge, connection) => {
            if (editable && workflow)
              state.command({
                type: 'edge.reconnect',
                workflowId: workflow.id,
                edgeId: edge.id,
                from: connection.source,
                to: connection.target,
                sourceHandle: (connection.sourceHandle ?? 'right') as 'left' | 'right' | 'top' | 'bottom',
                targetHandle: (connection.targetHandle ?? 'left') as 'left' | 'right' | 'top' | 'bottom',
              });
          }}
          nodesDraggable={editable}
          snapToGrid={editable}
          snapGrid={[10, 10]}
          nodesFocusable
          elementsSelectable
          deleteKeyCode={null}
          edgesFocusable
          onEdgeClick={(_, edge) => {
            select(null);
            if (!useScenarioStore.getState().selectedId) setSelectedEdge(edge.id);
          }}
          onEdgesChange={(changes) => {
            const change = changes.find((change) => change.type === 'select' && change.selected);
            if (change?.type === 'select') {
              select(null);
              if (!useScenarioStore.getState().selectedId) setSelectedEdge(change.id);
            }
          }}
          onNodeDragStop={() => useScenarioStore.getState().endHistoryGroup()}
          onNodeClick={(_, node) => {
            setSelectedEdge(null);
            select(node.id);
          }}
          onPaneClick={() => {
            setSelectedEdge(null);
            select(null);
          }}
          onNodesChange={(changes) => {
            const permitted = editable
              ? changes.filter((change) => change.type !== 'remove')
              : changes.filter((change) => change.type === 'select' || change.type === 'dimensions');
            setNodes((current) => applyNodeChanges(permitted, current));
            for (const change of changes) {
              if (editable && change.type === 'position' && change.position) move(change.id, change.position);
              if (change.type === 'select' && change.selected) {
                setSelectedEdge(null);
                select(change.id);
              }
            }
          }}
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
