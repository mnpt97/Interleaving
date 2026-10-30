import { useRef } from 'react';
import { Button } from '../../components/ui/button';
import { Dialog, DialogContent, DialogTitle, DialogDescription } from '../../components/ui/dialog';
import { desktopAvailable } from '../../lib/desktop';
import { useScenarioStore } from './store';
export function UnsavedChangesDialog() {
  const state = useScenarioStore();
  const previousFocus = useRef<HTMLElement | null>(null);
  const writing = state.busy || state.saving;
  return (
    <Dialog
      open={state.pendingChange}
      onOpenChange={(open) => {
        if (!open && !writing) void state.resolveChanges('cancel');
      }}
    >
      <DialogContent
        className="unsaved-dialog"
        onOpenAutoFocus={() => {
          previousFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          if (previousFocus.current?.isConnected) previousFocus.current.focus();
        }}
        dismissDisabled={writing}
        onEscapeKeyDown={(e) => {
          if (writing) e.preventDefault();
        }}
        onPointerDownOutside={(e) => {
          if (writing) e.preventDefault();
        }}
      >
        <DialogTitle>Save your changes?</DialogTitle>
        <DialogDescription>
          {Object.keys(state.formErrors).length
            ? 'Some fields are unfinished. Correct them before saving, or discard your changes.'
            : 'Your scenario has unsaved changes.'}
        </DialogDescription>
        {state.error && <p role="alert">{state.error}</p>}
        <div className="dialog-actions">
          <Button variant="outline" disabled={writing} onClick={() => void state.resolveChanges('discard')}>
            Discard
          </Button>
          <Button
            disabled={!desktopAvailable || writing || !state.dirty || Boolean(Object.keys(state.formErrors).length)}
            onClick={() => void state.resolveChanges('save')}
          >
            Save
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
