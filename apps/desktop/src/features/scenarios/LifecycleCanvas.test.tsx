import { act, cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { LifecycleCanvas, lifecycleNodes } from './LifecycleCanvas';
import { useScenarioStore } from './store';
import { executeCommand, newDraft } from './draft';
import { scopedId, localId } from './lifecycle';

vi.mock('../../lib/theme', () => ({ useTheme: () => ({ resolvedTheme: 'dark' }) }));

const initialState = useScenarioStore.getState();
beforeEach(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
  );
});
afterEach(() => {
  cleanup();
  useScenarioStore.setState(initialState, true);
  vi.unstubAllGlobals();
});

it('keeps real React Flow handle updates mounted after a scenario loads', () => {
  useScenarioStore.setState({ document: null, canvasView: 'lifecycle', mode: 'view' });
  const view = render(<LifecycleCanvas />);
  const document = executeCommand(newDraft(), { type: 'phase.node.add', phase: 'iteration', kind: 'http' });
  // Loading a document mounts ScenarioNode, whose effect calls useUpdateNodeInternals.
  // React Flow constructs this exact selector from the scoped ID.
  const id = scopedId('iteration', 'http-1');
  act(() => useScenarioStore.setState({ document }));
  expect(view.container.querySelector(`.react-flow__node[data-id="${id}"]`)).toBeInTheDocument();
});

it.each(['health', 'a"b', 'a\\b', 'a]b', 'a%b', '日本語'])('round-trips selector-safe scoped IDs: %s', (nodeId) => {
  const id = scopedId('iteration', nodeId);
  const element = document.createElement('div');
  element.setAttribute('data-id', id);
  const parent = document.createElement('div');
  parent.append(element);
  expect(parent.querySelector(`[data-id="${id}"]`)).toBe(element);
  expect(localId(id)).toEqual({ phase: 'iteration', id: nodeId });
});

it('renders safely before the initial scenario has loaded, including layout effects', () => {
  useScenarioStore.setState({ document: null });
  const view = render(<LifecycleCanvas />);
  expect(view.container).toBeEmptyDOMElement();
  expect(lifecycleNodes([], 'iteration', null)).toEqual([]);
});
