import { useEffect, useRef, useState } from 'react';

type Props = {
  label: string;
  controls: string;
  orientation: 'vertical' | 'horizontal';
  value: number;
  min: number;
  max: number;
  reverse?: boolean;
  onChange: (value: number) => void;
  onReset: () => void;
};

/** A keyboard-accessible splitter. Pointer capture keeps dragging over the canvas. */
export function PanelResizeHandle({
  label,
  controls,
  orientation,
  value,
  min,
  max,
  reverse,
  onChange,
  onReset,
}: Props) {
  const drag = useRef<{ pointer: number; coordinate: number; value: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const vertical = orientation === 'vertical';
  const direction = reverse ? -1 : 1;
  const change = (next: number) => onChange(Math.round(Math.max(min, Math.min(max, next))));

  useEffect(() => {
    if (!dragging) return;
    const { cursor, userSelect } = document.body.style;
    document.body.style.cursor = vertical ? 'col-resize' : 'row-resize';
    document.body.style.userSelect = 'none';
    return () => {
      document.body.style.cursor = cursor;
      document.body.style.userSelect = userSelect;
    };
  }, [dragging, vertical]);

  const stop = () => {
    drag.current = null;
    setDragging(false);
  };

  return (
    <div
      role="separator"
      tabIndex={0}
      aria-label={label}
      aria-controls={controls}
      aria-orientation={orientation}
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={value}
      aria-valuetext={`${value} pixels`}
      title={`${label} · Drag or use arrow keys. Double-click to reset.`}
      className={`panel-resize-handle ${orientation}${dragging ? ' dragging' : ''}`}
      onDoubleClick={onReset}
      onKeyDown={(event) => {
        const step = event.shiftKey ? 40 : 10;
        const decrease = vertical ? 'ArrowLeft' : 'ArrowUp';
        const increase = vertical ? 'ArrowRight' : 'ArrowDown';
        if (event.key === decrease || event.key === increase) {
          event.preventDefault();
          change(value + (event.key === increase ? step : -step) * direction);
        } else if (event.key === 'Home' || event.key === 'End') {
          event.preventDefault();
          change(event.key === 'Home' ? min : max);
        }
      }}
      onPointerDown={(event) => {
        if (!event.isPrimary || event.button !== 0) return;
        event.preventDefault();
        event.currentTarget.focus();
        event.currentTarget.setPointerCapture(event.pointerId);
        drag.current = { pointer: event.pointerId, coordinate: vertical ? event.clientX : event.clientY, value };
        setDragging(true);
      }}
      onPointerMove={(event) => {
        const start = drag.current;
        if (!start || start.pointer !== event.pointerId) return;
        const coordinate = vertical ? event.clientX : event.clientY;
        change(start.value + (coordinate - start.coordinate) * direction);
      }}
      onPointerUp={stop}
      onPointerCancel={stop}
      onLostPointerCapture={stop}
    />
  );
}
