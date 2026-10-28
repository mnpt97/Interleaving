import { ConnectionMode, Position, XYHandle, type InternalNodeBase } from '@xyflow/system';
import { expect, it, vi } from 'vitest';
import { executeCommand, newDraft } from './draft';
import { addNode, connect, connectionError } from './graph';

it.each([
  ['left', Position.Left],
  ['right', Position.Right],
  ['top', Position.Top],
  ['bottom', Position.Bottom],
] as const)('accepts a pointer drop on the diamond %s corner from an already connected HTTP node', (side, position) => {
  const draft = executeCommand(newDraft(), { type: 'http.insert', workflowId: 'workflow-1' });
  const workflow = draft.workflows[0];
  addNode(draft, workflow, 'exclusiveSplit', 'http-1');
  const diamond = workflow.nodes.find((node) => node.type === 'exclusiveSplit')!;
  const existingEdges = structuredClone(workflow.edges);
  const handle = document.createElement('div');
  handle.className = 'react-flow__handle target connectable connectableend';
  handle.dataset.nodeid = diamond.id;
  handle.dataset.handleid = side;
  handle.dataset.id = `test-${diamond.id}-${side}-target`;
  document.body.append(handle);
  const node: InternalNodeBase = {
    id: diamond.id,
    position: { x: 300, y: 100 },
    data: {},
    measured: { width: 64, height: 64 },
    internals: {
      positionAbsolute: { x: 300, y: 100 },
      z: 0,
      userNode: { id: diamond.id, position: { x: 300, y: 100 }, data: {} },
      handleBounds: {
        source: [],
        target: [{ id: side, type: 'target', nodeId: diamond.id, position, x: 0, y: 25, width: 14, height: 14 }],
      },
    },
  };
  // jsdom has no hit testing: supply the actual handle under the pointer.
  const original = Object.getOwnPropertyDescriptor(document, 'elementFromPoint');
  Object.defineProperty(document, 'elementFromPoint', { configurable: true, value: () => handle });
  try {
    const validate = vi.fn(
      (connection: { source: string; target: string }) =>
        !connectionError(workflow, connection.source, connection.target)
    );
    const result = XYHandle.isValid(new MouseEvent('mousemove', { clientX: 300, clientY: 132 }), {
      handle: { nodeId: diamond.id, id: side, type: 'target' },
      connectionMode: ConnectionMode.Strict,
      fromNodeId: 'http-1',
      fromHandleId: 'right',
      fromType: 'source',
      doc: document,
      lib: 'react',
      flowId: 'test',
      nodeLookup: new Map([[diamond.id, node]]),
      isValidConnection: validate,
    });
    expect(validate).toHaveBeenCalled();
    expect(result.isValid).toBe(true);
    expect(result.toHandle).toMatchObject({ nodeId: diamond.id, id: side, type: 'target' });
    connect(workflow, 'http-1', diamond.id, undefined, 'right', side);
    expect(workflow.edges.slice(0, existingEdges.length)).toEqual(existingEdges);
    const edge = workflow.edges[workflow.edges.length - 1];
    expect(workflow.layout!.connections![edge.id]).toEqual({ source: 'right', target: side });
  } finally {
    handle.remove();
    if (original) Object.defineProperty(document, 'elementFromPoint', original);
    else Reflect.deleteProperty(document, 'elementFromPoint');
  }
});
