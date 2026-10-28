import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { usePhaseLayout } from './usePhaseLayout';
import { executeCommand, newDraft } from './draft';
import { phaseBoxes, phaseWorkflow } from './lifecycle';

afterEach(() => {
  cleanup();
  localStorage.clear();
});

it('keeps user dimensions fixed when nodes move and restores them after reload', () => {
  const document = executeCommand(newDraft(), { type: 'phase.node.add', phase: 'iteration', kind: 'http' });
  const view = renderHook(({ document }) => usePhaseLayout(document.id, phaseBoxes(document)), {
    initialProps: { document },
  });
  act(() => view.result.current.resize('iteration', 1100, 500));
  act(() => view.result.current.move('iteration', { x: 400, y: 700 }));
  const moved = executeCommand(document, {
    type: 'node.move',
    workflowId: phaseWorkflow(document, 'iteration')!.id,
    nodeId: 'http-1',
    position: { x: 2500, y: 900 },
  });
  view.rerender({ document: moved });
  expect(view.result.current.boxes[1]).toMatchObject({ width: 1100, height: 500, x: 400, y: 700 });
  act(() => view.result.current.resize('iteration', 1200, 600));
  expect(view.result.current.boxes[1]).toMatchObject({ x: 400, y: 700 });
  view.unmount();
  const reloaded = renderHook(() => usePhaseLayout(document.id, phaseBoxes(moved)));
  expect(reloaded.result.current.boxes[1]).toMatchObject({ width: 1200, height: 600, x: 400, y: 700 });
});

it('isolates preferences between scenarios and survives unavailable storage', () => {
  const document = newDraft();
  localStorage.setItem(`interleaving.phase-layout:${document.id}`, 'broken JSON');
  const view = renderHook(({ id }) => usePhaseLayout(id, phaseBoxes(document)), { initialProps: { id: document.id } });
  act(() => view.result.current.resize('initialize', 900, 700));
  view.rerender({ id: 'other' });
  expect(view.result.current.boxes[0].height).toBe(180);
});

it('replaces cached automatic sizes from older builds with content bounds', () => {
  const document = newDraft();
  localStorage.setItem(
    `interleaving.phase-layout:${document.id}`,
    JSON.stringify({
      initialize: { width: 640, height: 360, minX: 0, minY: 0 },
      iteration: { width: 1000, height: 500, minX: 0, minY: 0 },
      cleanup: { width: 640, height: 360, minX: 0, minY: 0, x: 500, y: 900 },
    })
  );
  const view = renderHook(() => usePhaseLayout(document.id, phaseBoxes(document)));
  expect(view.result.current.boxes[0]).toMatchObject({ width: 320, height: 180 });
  expect(view.result.current.boxes[1]).toMatchObject({ width: 1000, height: 500 });
  expect(view.result.current.boxes[2]).toMatchObject({ width: 320, height: 180, x: 500, y: 900 });
});

it('does not persist automatic dimensions as manual preferences', () => {
  const document = newDraft();
  const view = renderHook(() => usePhaseLayout(document.id, phaseBoxes(document)));
  expect(JSON.parse(localStorage.getItem(`interleaving.phase-layout:${document.id}`)!).sizes).toEqual({});
  act(() => view.result.current.move('initialize', { x: 500, y: 600 }));
  view.unmount();
  const next = executeCommand(document, { type: 'phase.node.add', phase: 'initialize', kind: 'http' });
  const reloaded = renderHook(() => usePhaseLayout(next.id, phaseBoxes(next)));
  expect(reloaded.result.current.boxes[0]).toMatchObject({ width: 350, height: 240, x: 500, y: 600 });
});

it('centers default boxes using their current widths while retaining manually moved positions', () => {
  const document = newDraft();
  const view = renderHook(() => usePhaseLayout(document.id, phaseBoxes(document)));
  act(() => view.result.current.resize('iteration', 1000, 400));
  expect(new Set(view.result.current.boxes.map((box) => box.x + box.width / 2)).size).toBe(1);
  act(() => view.result.current.move('initialize', { x: 120, y: 140 }));
  act(() => view.result.current.resize('iteration', 1200, 400));
  expect(view.result.current.boxes[0].x).toBe(120);
  expect(view.result.current.boxes[1].x + view.result.current.boxes[1].width / 2).toBe(
    view.result.current.boxes[2].x + view.result.current.boxes[2].width / 2
  );
});
