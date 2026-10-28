import * as DialogPrimitive from '@radix-ui/react-dialog';
import type { ComponentProps } from 'react';
import { X } from 'lucide-react';
import { cn } from '../../lib/utils';
import { IconTooltip } from './tooltip';
export const Dialog = DialogPrimitive.Root;
export const DialogTitle = DialogPrimitive.Title;
export const DialogDescription = DialogPrimitive.Description;
export function DialogContent({
  className,
  children,
  dismissDisabled = false,
  ...props
}: ComponentProps<typeof DialogPrimitive.Content> & { dismissDisabled?: boolean }) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="dialog-overlay" />
      <DialogPrimitive.Content data-slot="dialog-content" className={cn('dialog-content', className)} {...props}>
        {children}
        <IconTooltip className="dialog-close-tooltip" label="Close and keep editing" disabled={dismissDisabled}>
          <DialogPrimitive.Close className="dialog-close" aria-label="Close" disabled={dismissDisabled}>
            <X size={18} />
          </DialogPrimitive.Close>
        </IconTooltip>
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}
