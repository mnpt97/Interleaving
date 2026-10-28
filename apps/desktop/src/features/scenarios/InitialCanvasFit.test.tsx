import { render } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { InitialCanvasFit } from './InitialCanvasFit';

const flow = vi.hoisted(() => ({ measured: false, ready: false, fitView: vi.fn() }));
vi.mock('@xyflow/react', () => ({
  useNodesInitialized: () => flow.measured,
  useReactFlow: () => ({ fitView: flow.fitView, viewportInitialized: flow.ready }),
}));
beforeEach(() => {
  flow.measured = false;
  flow.ready = false;
  flow.fitView.mockClear();
});

it('waits for viewport and node measurements, then fits only once per loaded scenario', () => {
  const view = render(<InitialCanvasFit sceneKey="first" />);
  flow.ready = true;
  view.rerender(<InitialCanvasFit sceneKey="first" />);
  expect(flow.fitView).not.toHaveBeenCalled();
  flow.measured = true;
  view.rerender(<InitialCanvasFit sceneKey="first" />);
  expect(flow.fitView).toHaveBeenCalledExactlyOnceWith({ padding: 0.12, minZoom: 0.01, maxZoom: 1, duration: 0 });
  // A drag or newly measured node must not reset the user's manual viewport.
  flow.measured = false;
  view.rerender(<InitialCanvasFit sceneKey="first" />);
  flow.measured = true;
  view.rerender(<InitialCanvasFit sceneKey="first" />);
  expect(flow.fitView).toHaveBeenCalledTimes(1);
  view.rerender(<InitialCanvasFit sceneKey="second" />);
  expect(flow.fitView).toHaveBeenCalledTimes(2);
});
