import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';

export const WORKSPACE_LAYOUT_KEY = 'interleaving.workspace-layout';
const defaults = { workflow: 204, inspector: 268, bottom: 244 };
type Sizes = typeof defaults;

function readSizes(): Sizes {
  try {
    const saved = JSON.parse(localStorage.getItem(WORKSPACE_LAYOUT_KEY) ?? 'null');
    return Object.fromEntries(
      Object.entries(defaults).map(([key, fallback]) => {
        const value: unknown = saved?.[key];
        return [
          key,
          typeof value === 'number' && Number.isFinite(value) ? Math.max(120, Math.min(1200, value)) : fallback,
        ];
      })
    ) as Sizes;
  } catch {
    return defaults;
  }
}

/** Keep preferred dimensions separate from viewport clamps, so resizing the window
 * or hiding the inspector does not overwrite the user's saved layout. */
export function useWorkspaceLayout(inspectorOpen: boolean) {
  const ref = useRef<HTMLDivElement>(null);
  const [preferred, setPreferred] = useState(readSizes);
  const [bounds, setBounds] = useState({ width: window.innerWidth, height: Math.max(304, window.innerHeight - 180) });
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const measure = () => {
      const rect = element.getBoundingClientRect();
      if (rect.width && rect.height) setBounds({ width: rect.width, height: rect.height });
    };
    measure();
    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', measure);
      return () => window.removeEventListener('resize', measure);
    }
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    try {
      localStorage.setItem(WORKSPACE_LAYOUT_KEY, JSON.stringify(preferred));
    } catch {
      // A denied/full preference store must not prevent resizing this session.
    }
  }, [preferred]);

  const inspector = Math.round(
    Math.min(Math.max(220, preferred.inspector), Math.max(220, Math.min(640, bounds.width - 160 - 280 - 8)))
  );
  const workflowMax = Math.max(160, Math.min(480, bounds.width - (inspectorOpen ? inspector + 8 : 4) - 280));
  const workflow = Math.round(Math.min(Math.max(160, preferred.workflow), workflowMax));
  const inspectorMax = Math.max(220, Math.min(640, bounds.width - workflow - 280 - 8));
  const bottomMax = Math.max(120, Math.round(bounds.height - 180 - 4));
  const bottom = Math.round(Math.min(Math.max(120, preferred.bottom), bottomMax));
  const set = (key: keyof Sizes) => (value: number) => setPreferred((current) => ({ ...current, [key]: value }));
  const reset = (key: keyof Sizes) => () => set(key)(defaults[key]);
  const style = {
    '--workflow-width': `${workflow}px`,
    '--inspector-width': `${inspector}px`,
    '--bottom-height': `${bottom}px`,
  } as CSSProperties;
  return {
    ref,
    style,
    workflow: { value: workflow, min: 160, max: workflowMax, onChange: set('workflow'), onReset: reset('workflow') },
    inspector: {
      value: inspector,
      min: 220,
      max: inspectorMax,
      onChange: set('inspector'),
      onReset: reset('inspector'),
    },
    bottom: { value: bottom, min: 120, max: bottomMax, onChange: set('bottom'), onReset: reset('bottom') },
  };
}
