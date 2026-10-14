import { applyNodeChanges, Background, Controls, Handle, Position, ReactFlow, useNodesState, type NodeProps, type NodeTypes } from "@xyflow/react";
import { CirclePlay, Flag, Globe } from "lucide-react";
import { useEffect, useMemo } from "react";
import { toFlowEdges, toFlowNodes, type ScenarioFlowNode } from "./adapters";
import { useScenarioStore } from "./store";
import "@xyflow/react/dist/style.css";
import { useTheme } from "../../lib/theme";
import { useRunStore } from "../runs/store";

function ScenarioNode({ data, selected }: NodeProps<ScenarioFlowNode>) {
  const node = data.node;
  const scenarioId = useScenarioStore((s) => s.document?.id);
  const runStatus = useRunStore((s) => {
    if (s.scenarioId !== scenarioId) return null;
    const last = [...s.events].reverse().find((event) => event.nodeId === node.id && ["step.started", "step.finished", "step.failed"].includes(event.type));
    return last?.type === "step.started" ? "Running" : last?.type === "step.finished" ? "Completed" : last?.type === "step.failed" ? "Stopped" : null;
  });
  const Icon = node.type === "start" ? CirclePlay : node.type === "end" ? Flag : Globe;
  return <div className={`scenario-node ${selected ? "is-selected" : ""}`}>
    {node.type !== "start" && <Handle type="target" position={Position.Left} />}
    <div className="node-heading"><Icon size={16} /><span>{node.type === "http" ? "HTTP request" : node.type === "start" ? "Start" : "End"}</span></div>
    <strong>{node.id}</strong>
    {runStatus && <span className="node-run-status">{runStatus}</span>}
    {node.type === "http" && <div className="request-summary"><span>{node.config.method}</span><code>{node.config.path}</code></div>}
    {node.type !== "end" && <Handle type="source" position={Position.Right} />}
  </div>;
}
const nodeTypes: NodeTypes = { scenario: ScenarioNode };

export function ScenarioCanvas() {
  const { resolvedTheme } = useTheme();
  const document = useScenarioStore((s) => s.document);
  const selected = useScenarioStore((s) => s.selectedId);
  const select = useScenarioStore((s) => s.select);
  const move = useScenarioStore((s) => s.move);
  // Measurements and selection are transient React Flow state, never domain data.
  const [nodes, setNodes] = useNodesState<ScenarioFlowNode>(document ? toFlowNodes(document, selected) : []);
  useEffect(() => {
    setNodes((current) => document ? toFlowNodes(document, selected).map((node) => ({ ...current.find((previous) => previous.id === node.id), ...node })) : []);
  }, [document, selected, setNodes]);
  const edges = useMemo(() => document ? toFlowEdges(document) : [], [document]);
  if (!document) return <div className="canvas-empty"><h2>Unable to render this scenario</h2><p>Review the diagnostics, update the file, then open it again.</p></div>;
  return <ReactFlow proOptions={{ hideAttribution: true }} colorMode={resolvedTheme} nodes={nodes} edges={edges} nodeTypes={nodeTypes} fitView fitViewOptions={{ padding: 0.3 }} minZoom={0.35} maxZoom={1.5} nodesConnectable={false} nodesDraggable elementsSelectable deleteKeyCode={null} edgesFocusable={false} onNodeClick={(_, node) => select(node.id)} onPaneClick={() => select(null)} onNodesChange={(changes) => { setNodes((current) => applyNodeChanges(changes, current)); for (const change of changes) if (change.type === "position" && change.position) move(change.id, change.position); }}>
    <Background gap={24} size={1} color="var(--canvas-grid)" />
    <Controls showInteractive={false} />
  </ReactFlow>;
}
