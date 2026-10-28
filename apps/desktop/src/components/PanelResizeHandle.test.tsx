import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PanelResizeHandle } from './PanelResizeHandle';
import { useWorkspaceLayout, WORKSPACE_LAYOUT_KEY } from './useWorkspaceLayout';

function Layout({ inspectorOpen = true }: { inspectorOpen?: boolean }) {
  const layout = useWorkspaceLayout(inspectorOpen);
  return (
    <div ref={layout.ref} style={layout.style} data-testid="layout">
      <PanelResizeHandle label="Workflow" controls="workflow" orientation="vertical" {...layout.workflow} />
      {inspectorOpen && (
        <PanelResizeHandle
          label="Inspector"
          controls="inspector"
          orientation="vertical"
          reverse
          {...layout.inspector}
        />
      )}
      <PanelResizeHandle label="Results" controls="results" orientation="horizontal" reverse {...layout.bottom} />
    </div>
  );
}

let resize: () => void;
let bounds = { width: 1200, height: 600 };
beforeEach(() => {
  localStorage.removeItem(WORKSPACE_LAYOUT_KEY);
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(() => ({
    ...bounds,
    x: 0,
    y: 0,
    top: 0,
    left: 0,
    right: bounds.width,
    bottom: bounds.height,
    toJSON: () => ({}),
  }));
  vi.stubGlobal(
    'ResizeObserver',
    class {
      constructor(callback: () => void) {
        resize = callback;
      }
      observe() {}
      disconnect() {}
    }
  );
});
afterEach(() => {
  bounds = { width: 1200, height: 600 };
  localStorage.removeItem(WORKSPACE_LAYOUT_KEY);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('workspace resizing', () => {
  it('resizes each panel with correctly directed arrow keys, respects limits, and resets', () => {
    render(<Layout />);
    const workflow = screen.getByRole('separator', { name: 'Workflow' });
    const inspector = screen.getByRole('separator', { name: 'Inspector' });
    const results = screen.getByRole('separator', { name: 'Results' });
    fireEvent.keyDown(workflow, { key: 'ArrowRight' });
    fireEvent.keyDown(inspector, { key: 'ArrowLeft', shiftKey: true });
    fireEvent.keyDown(results, { key: 'ArrowUp' });
    expect(workflow).toHaveAttribute('aria-valuenow', '214');
    expect(inspector).toHaveAttribute('aria-valuenow', '308');
    expect(results).toHaveAttribute('aria-valuenow', '254');
    fireEvent.keyDown(results, { key: 'End' });
    expect(results).toHaveAttribute('aria-valuenow', '416');
    fireEvent.keyDown(results, { key: 'ArrowUp' });
    expect(results).toHaveAttribute('aria-valuenow', '416');
    fireEvent.doubleClick(results);
    expect(results).toHaveAttribute('aria-valuenow', '244');
  });

  it('captures pointer drags and restores the cursor when dragging is cancelled', () => {
    vi.stubGlobal(
      'PointerEvent',
      class extends MouseEvent {
        pointerId: number;
        isPrimary = true;
        constructor(type: string, init: PointerEventInit) {
          super(type, init);
          this.pointerId = init.pointerId ?? 1;
        }
      }
    );
    render(<Layout />);
    const handle = screen.getByRole('separator', { name: 'Inspector' });
    handle.setPointerCapture = vi.fn();
    fireEvent.pointerDown(handle, { pointerId: 1, button: 0, clientX: 900 });
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 820 });
    expect(handle).toHaveAttribute('aria-valuenow', '348');
    expect(document.body.style.cursor).toBe('col-resize');
    fireEvent.pointerCancel(handle, { pointerId: 1 });
    expect(document.body.style.cursor).toBe('');
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 800 });
    expect(handle).toHaveAttribute('aria-valuenow', '348');
  });

  it('preserves preferred dimensions across viewport clamps, inspector toggles, and remounts', () => {
    const { rerender, unmount } = render(<Layout />);
    const workflow = screen.getByRole('separator', { name: 'Workflow' });
    fireEvent.keyDown(workflow, { key: 'End' });
    expect(workflow).toHaveAttribute('aria-valuenow', '480');
    bounds = { width: 800, height: 400 };
    act(() => resize());
    expect(workflow).toHaveAttribute('aria-valuenow', '244');
    bounds = { width: 1200, height: 600 };
    act(() => resize());
    expect(workflow).toHaveAttribute('aria-valuenow', '480');
    rerender(<Layout inspectorOpen={false} />);
    expect(screen.queryByRole('separator', { name: 'Inspector' })).not.toBeInTheDocument();
    rerender(<Layout />);
    expect(screen.getByRole('separator', { name: 'Inspector' })).toHaveAttribute('aria-valuenow', '268');
    unmount();
    render(<Layout />);
    expect(screen.getByRole('separator', { name: 'Workflow' })).toHaveAttribute('aria-valuenow', '480');
  });

  it('ignores corrupt saved preferences and remains usable when storage is unavailable', () => {
    localStorage.setItem(WORKSPACE_LAYOUT_KEY, '{broken');
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('unavailable');
    });
    render(<Layout />);
    const results = screen.getByRole('separator', { name: 'Results' });
    fireEvent.keyDown(results, { key: 'ArrowUp' });
    expect(results).toHaveAttribute('aria-valuenow', '254');
  });
});
