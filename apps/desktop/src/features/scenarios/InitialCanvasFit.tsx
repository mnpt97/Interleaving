import { useEffect, useRef } from 'react';
import { useNodesInitialized, useReactFlow } from '@xyflow/react';

/** Fit once after both the viewport and every asynchronously added node are measured. */
export function InitialCanvasFit({ sceneKey }: { sceneKey: string }) {
  const initialized = useNodesInitialized();
  const { fitView, viewportInitialized } = useReactFlow();
  const fitted = useRef<string | null>(null);
  useEffect(() => {
    if (!initialized || !viewportInitialized || fitted.current === sceneKey) return;
    fitted.current = sceneKey;
    void fitView({ padding: 0.12, minZoom: 0.01, maxZoom: 1, duration: 0 });
  }, [initialized, viewportInitialized, sceneKey, fitView]);
  return null;
}
