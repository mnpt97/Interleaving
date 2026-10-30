import { useEffect } from 'react';
import { desktopAvailable } from '../../lib/desktop';
import { useScenarioStore } from './store';

/** Debounce accepted domain edits; transient fragments never cross the bridge. */
export function useDraftValidation() {
  const state = useScenarioStore();
  const blocked = Object.keys(state.formErrors).length > 0;
  useEffect(() => {
    if (
      !desktopAvailable ||
      state.mode !== 'edit' ||
      state.status !== 'unchecked' ||
      state.busy ||
      state.saving ||
      blocked
    )
      return;
    const revision = state.revision;
    const timer = window.setTimeout(() => {
      const current = useScenarioStore.getState();
      if (current.revision === revision && current.mode === 'edit' && !Object.keys(current.formErrors).length)
        void current.validate();
    }, 500);
    return () => window.clearTimeout(timer);
  }, [state.revision, state.mode, state.status, state.busy, state.saving, blocked]);
}
