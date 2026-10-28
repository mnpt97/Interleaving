import * as TooltipPrimitive from '@radix-ui/react-tooltip';
import type { ComponentProps, ReactElement } from 'react';
import { cn } from '../../lib/utils';

export const TooltipProvider = TooltipPrimitive.Provider;
export const Tooltip = TooltipPrimitive.Root;
export const TooltipTrigger = TooltipPrimitive.Trigger;

export function TooltipContent({
  className,
  sideOffset = 6,
  ...props
}: ComponentProps<typeof TooltipPrimitive.Content>) {
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content
        data-slot="tooltip-content"
        sideOffset={sideOffset}
        className={cn('icon-tooltip', className)}
        {...props}
      />
    </TooltipPrimitive.Portal>
  );
}

/** The wrapper keeps help available when the underlying button is disabled. */
export function IconTooltip({
  children,
  label,
  disabled = false,
  className,
}: {
  children: ReactElement;
  label: string;
  disabled?: boolean;
  className?: string;
}) {
  return (
    <TooltipProvider delayDuration={350}>
      <Tooltip>
        <TooltipTrigger asChild>
          <span className={cn('icon-tooltip-trigger', className)} tabIndex={disabled ? 0 : undefined}>
            {children}
          </span>
        </TooltipTrigger>
        <TooltipContent>{label}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
