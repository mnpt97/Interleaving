import { describe, expect, it } from 'vitest';
import { moveNode, toFlowEdges, toFlowNodes } from './adapters';
import { exampleDocument } from './store';

describe('React Flow adapters', () => {
  it('keeps layout updates out of executable workflow and leaves input immutable', () => {
    const original = structuredClone(exampleDocument);
    const moved = moveNode(original, 'health', { x: 120, y: 240 });
    expect(moved.workflows[0].nodes).toBe(original.workflows[0].nodes);
    expect(moved.workflows[0].edges).toBe(original.workflows[0].edges);
    expect(original).toEqual(exampleDocument);
    expect(toFlowNodes(moved, 'health').find((n) => n.id === 'health')).toMatchObject({
      selected: true,
      position: { x: 120, y: 240 },
    });
    expect(toFlowEdges(moved)).toEqual(toFlowEdges(original));
    expect(moved.workflows[0].edges[0]).toHaveProperty('id', 'edge-0');
  });
  it('supplies deterministic positions when layout is absent', () => {
    const document = { ...exampleDocument, workflows: [{ ...exampleDocument.workflows[0], layout: undefined }] };
    expect(toFlowNodes(document, null).map((node) => node.position)).toEqual([
      { x: 80, y: 160 },
      { x: 340, y: 160 },
      { x: 600, y: 160 },
    ]);
    expect(toFlowNodes(document, null).map((node) => node.origin)).toEqual([
      [0, 0.5],
      [0, 0.5],
      [0, 0.5],
    ]);
  });
  it('rounds saved coordinates and ignores movement within the same hundredth', () => {
    const moved = moveNode(exampleDocument, 'health', { x: 120.12345, y: -240.98765 });
    expect(moved.workflows[0].layout?.positions.health).toEqual({ x: 120.12, y: -240.99 });
    expect(moveNode(moved, 'health', { x: 120.121, y: -240.991 })).toBe(moved);
  });
  it('rejects nonfinite positions and unknown node IDs', () => {
    expect(moveNode(exampleDocument, 'missing', { x: 1, y: 1 })).toBe(exampleDocument);
    expect(moveNode(exampleDocument, 'health', { x: NaN, y: 1 })).toBe(exampleDocument);
  });
});
