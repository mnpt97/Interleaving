import { describe, expect, it } from 'vitest';
import { executeCommand, newDraft } from './draft';
import { ownsPhase, phaseBoxes, phaseWorkflow, phases, scopedId, localId } from './lifecycle';
import { documentSchema } from '../../lib/scenario-types';
import showcase from '../../../../../scenarios/examples/feature-showcase.interl.json?raw';

function add(phase: 'initialize' | 'iteration' | 'cleanup') {
  return executeCommand(newDraft(), { type: 'phase.node.add', phase, kind: 'http' });
}
describe('lifecycle authoring', () => {
  it('keeps the showcase Initialize and Cleanup boxes smaller than Iteration', () => {
    const boxes = phaseBoxes(documentSchema.parse(JSON.parse(showcase)));
    expect(boxes[0].width).toBeLessThan(boxes[1].width);
    expect(boxes[2].width).toBeLessThan(boxes[1].width);
    expect(boxes[0].height).toBeLessThan(boxes[1].height);
    expect(boxes[2].height).toBeLessThan(boxes[1].height);
    expect(new Set(boxes.map((box) => box.x + box.width / 2)).size).toBe(1);
  });
  it('fits each phase to its own content instead of the widest phase', () => {
    let document = add('iteration');
    document = executeCommand(document, { type: 'phase.node.add', phase: 'initialize', kind: 'http' });
    document = executeCommand(document, { type: 'phase.node.add', phase: 'iteration', kind: 'http' });
    const boxes = phaseBoxes(document);
    expect(boxes[0].width).toBe(350);
    expect(boxes[0].height).toBe(240);
    expect(boxes[1].width).toBeGreaterThan(boxes[0].width);
    expect(boxes[2]).toMatchObject({ width: 320, height: 180 });
  });
  it('projects three phases without changing or creating optional bindings', () => {
    const document = newDraft(),
      before = JSON.stringify(document);
    expect(phaseBoxes(document).map((box) => box.phase)).toEqual(phases);
    const boxes = phaseBoxes(document);
    expect(new Set(boxes.map((box) => box.x)).size).toBe(1);
    expect(new Set(boxes.map((box) => box.width)).size).toBe(1);
    expect(boxes[1].y).toBeGreaterThan(boxes[0].y + boxes[0].height);
    expect(boxes[2].y).toBeGreaterThan(boxes[1].y + boxes[1].height);
    expect(JSON.stringify(document)).toBe(before);
    expect(document.groups[0].lifecycle.initialize).toBeNull();
    expect(document.groups[0].lifecycle.cleanup).toBeNull();
  });
  it('creates phase-owned definitions directly and keeps duplicate local IDs scoped', () => {
    let document = add('initialize');
    document = executeCommand(document, { type: 'phase.node.add', phase: 'cleanup', kind: 'http' });
    const init = phaseWorkflow(document, 'initialize')!,
      cleanup = phaseWorkflow(document, 'cleanup')!;
    expect(init.id).not.toBe(cleanup.id);
    expect(init.nodes.some((node) => node.id === 'http-1')).toBe(true);
    expect(cleanup.nodes.some((node) => node.id === 'http-1')).toBe(true);
    expect(scopedId('initialize', 'http-1')).not.toBe(scopedId('cleanup', 'http-1'));
    expect(localId(scopedId('cleanup', 'http-1'))).toEqual({ phase: 'cleanup', id: 'http-1' });
    expect(documentSchema.parse(JSON.parse(JSON.stringify(document)))).toEqual(document);
  });
  it('preserves shared definitions and detaches a deliberate independent copy', () => {
    const original = add('initialize');
    original.groups[0].lifecycle.cleanup = structuredClone(original.groups[0].lifecycle.initialize);
    expect(ownsPhase(original, 'initialize')).toBe(false);
    expect(() => executeCommand(original, { type: 'phase.node.add', phase: 'initialize', kind: 'http' })).toThrow(
      /shared/
    );
    const detached = executeCommand(original, { type: 'phase.detach', phase: 'initialize' });
    expect(phaseWorkflow(detached, 'initialize')!.id).not.toBe(phaseWorkflow(detached, 'cleanup')!.id);
    expect(phaseWorkflow(detached, 'cleanup')).toEqual(phaseWorkflow(original, 'cleanup'));
    expect(ownsPhase(detached, 'initialize')).toBe(true);
  });
  it('moves a closed set atomically, unbinding an emptied optional phase without wiring new edges', () => {
    const original = add('initialize');
    const moved = executeCommand(original, {
      type: 'phase.move',
      from: 'initialize',
      to: 'cleanup',
      nodeIds: ['http-1'],
    });
    expect(moved.groups[0].lifecycle.initialize).toBeNull();
    expect(phaseWorkflow(moved, 'cleanup')!.nodes.some((node) => node.id === 'http-1')).toBe(true);
    expect(phaseWorkflow(moved, 'cleanup')!.edges).toEqual([]);
    expect(original.groups[0].lifecycle.cleanup).toBeNull();
  });
  it('protects consumers, workflow inputs, exported outputs and destination IDs', () => {
    let document = add('initialize');
    const init = phaseWorkflow(document, 'initialize')!;
    init.outputs.result = { type: 'integer', value: { source: 'response', nodeId: 'http-1', field: 'status' } };
    expect(() =>
      executeCommand(document, { type: 'phase.move', from: 'initialize', to: 'cleanup', nodeIds: ['http-1'] })
    ).toThrow(/referenced/);
    delete init.outputs.result;
    init.nodes.push({
      id: 'session',
      type: 'setSession',
      config: { setHeaders: { 'x-value': { source: 'input', name: 'token', prefix: '' } }, removeHeaders: [] },
    });
    expect(() =>
      executeCommand(document, { type: 'phase.move', from: 'initialize', to: 'cleanup', nodeIds: ['session'] })
    ).toThrow(/input/);
    document = executeCommand(document, { type: 'phase.node.add', phase: 'cleanup', kind: 'http' });
    expect(() =>
      executeCommand(document, { type: 'phase.move', from: 'initialize', to: 'cleanup', nodeIds: ['http-1'] })
    ).toThrow(/ID/);
  });
  it('preserves local layout independently of phase bounds and rounds persisted coordinates', () => {
    const original = add('iteration');
    const flow = phaseWorkflow(original, 'iteration')!;
    const moved = executeCommand(original, {
      type: 'node.move',
      workflowId: flow.id,
      nodeId: 'http-1',
      position: { x: 123.456, y: 240.123 },
    });
    expect(phaseWorkflow(moved, 'iteration')!.layout!.positions['http-1']).toEqual({ x: 123.46, y: 240.12 });
    expect(moved.groups).toEqual(original.groups);
  });
});
