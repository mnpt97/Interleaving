import { hostStorageKey } from '../../lib/desktop';
import { useCallback, useEffect, useMemo, useState } from 'react';
import type { Phase, PhaseBox } from './lifecycle';

type Size = Pick<PhaseBox, 'width' | 'height' | 'minX' | 'minY'> &
  Partial<Pick<PhaseBox, 'x' | 'y'>> & { resized?: boolean };
type Sizes = Partial<Record<Phase, Size>>;
const storageKey = (id: string) => hostStorageKey(`interleaving.phase-layout:${id}`);
function read(id: string, defaults: PhaseBox[]): Sizes {
  try {
    const payload = JSON.parse(localStorage.getItem(storageKey(id)) ?? '{}');
    const modern = payload.version === 2;
    const saved = modern ? payload.sizes : payload;
    // Older builds also saved automatically calculated, equal-width boxes.
    const legacy = defaults.map((box) => {
      const positions = box.inline
        ? (box.workflow?.nodes.filter((node) => node.type !== 'start' && node.type !== 'end') ?? []).map(
            (node, index) => box.workflow?.layout?.positions[node.id] ?? { x: 340 + index * 260, y: 160 }
          )
        : [];
      const minX = Math.min(0, ...positions.map((point) => point.x - 120));
      const minY = Math.min(0, ...positions.map((point) => point.y - 120));
      return {
        phase: box.phase,
        width: Math.max(640, ...positions.map((point) => point.x - minX + 360)),
        height: Math.max(360, ...positions.map((point) => point.y - minY + 180)),
      };
    });
    const legacyWidth = Math.max(...legacy.map((box) => box.width));
    return Object.fromEntries(
      Object.entries(saved).flatMap(([phase, size]) => {
        const value = size as Size;
        const fallback = defaults.find((box) => box.phase === phase);
        if (
          !fallback ||
          !value ||
          ![value.width, value.height, value.minX, value.minY].every(Number.isFinite) ||
          value.width < 320 ||
          value.height < 180 ||
          (value.x !== undefined && !Number.isFinite(value.x)) ||
          (value.y !== undefined && !Number.isFinite(value.y))
        )
          return [];
        const oldDefault =
          !modern && value.width === legacyWidth && value.height === legacy.find((box) => box.phase === phase)?.height;
        const resized = modern ? value.resized === true : !oldDefault;
        const dimensions = resized
          ? value
          : { width: fallback.width, height: fallback.height, minX: fallback.minX, minY: fallback.minY };
        return [
          [
            phase,
            {
              ...dimensions,
              resized,
              ...(value.x === undefined ? {} : { x: value.x }),
              ...(value.y === undefined ? {} : { y: value.y }),
            },
          ],
        ];
      })
    );
  } catch {
    return {};
  }
}

/** Phase dimensions are local workspace preferences, independent of executable scenario fields. */
export function usePhaseLayout(id: string, defaults: PhaseBox[]) {
  const [saved, setSaved] = useState(() => ({ id, sizes: read(id, defaults) }));
  useEffect(() => {
    setSaved((current) => {
      const sizes = current.id === id ? current.sizes : read(id, defaults);
      const missing = defaults.filter((box) => !sizes[box.phase]);
      if (current.id === id && !missing.length) return current;
      return {
        id,
        sizes: {
          ...sizes,
          ...Object.fromEntries(
            missing.map((box) => [
              box.phase,
              {
                width: box.width,
                height: box.height,
                minX: box.minX,
                minY: box.minY,
              },
            ])
          ),
        },
      };
    });
  }, [id, defaults]);
  useEffect(() => {
    if (saved.id !== id || !id) return;
    try {
      localStorage.setItem(
        storageKey(id),
        JSON.stringify({
          version: 2,
          sizes: Object.fromEntries(
            Object.entries(saved.sizes).filter(
              ([, size]) => size.resized || size.x !== undefined || size.y !== undefined
            )
          ),
        })
      );
    } catch {
      /* Keep session preferences. */
    }
  }, [saved, id]);
  const boxes = useMemo(() => {
    const sizes = saved.id === id ? saved.sizes : {};
    let y = 140;
    const frames = defaults.map((box) => {
      const next = { ...box, y, ...sizes[box.phase] };
      y += next.height + 100;
      return next;
    });
    const widest = Math.max(...frames.map((box) => box.width));
    return frames.map((box) => ({ ...box, x: sizes[box.phase]?.x ?? 80 + (widest - box.width) / 2 }));
  }, [defaults, saved, id]);
  const resize = useCallback(
    (phase: Phase, width: number, height: number) => {
      if (![width, height].every(Number.isFinite)) return;
      const box = boxes.find((box) => box.phase === phase);
      if (!box) return;
      setSaved((current) => ({
        id,
        sizes: {
          ...(current.id === id ? current.sizes : {}),
          [phase]: {
            ...current.sizes[phase],
            minX: box.minX,
            minY: box.minY,
            resized: true,
            width: Math.max(320, Math.round(width)),
            height: Math.max(180, Math.round(height)),
          },
        },
      }));
    },
    [boxes, id]
  );
  const move = useCallback(
    (phase: Phase, position: { x: number; y: number }) => {
      if (![position.x, position.y].every(Number.isFinite)) return;
      const box = boxes.find((box) => box.phase === phase);
      if (!box) return;
      setSaved((current) => ({
        id,
        sizes: {
          ...current.sizes,
          [phase]: {
            resized: current.sizes[phase]?.resized,
            width: box.width,
            height: box.height,
            minX: box.minX,
            minY: box.minY,
            x: Math.round(position.x / 10) * 10,
            y: Math.round(position.y / 10) * 10,
          },
        },
      }));
    },
    [boxes, id]
  );
  return { boxes, resize, move };
}
