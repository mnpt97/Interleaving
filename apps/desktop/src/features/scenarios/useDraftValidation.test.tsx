import { act, render } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useDraftValidation } from './useDraftValidation';
import { useScenarioStore } from './store';
vi.mock('../../lib/desktop', async (original) => ({
  ...(await original<typeof import('../../lib/desktop')>()),
  desktopAvailable: true,
}));
const initial = useScenarioStore.getState();
function Validation() {
  useDraftValidation();
  return null;
}
beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  vi.useRealTimers();
  useScenarioStore.setState(initial, true);
});
it('debounces accepted semantic edits and skips uncommitted fragments', () => {
  const validate = vi.fn();
  useScenarioStore.setState({
    mode: 'edit',
    status: 'unchecked',
    busy: false,
    saving: false,
    formErrors: {},
    validate,
  });
  render(<Validation />);
  act(() => vi.advanceTimersByTime(400));
  expect(validate).not.toHaveBeenCalled();
  act(() => useScenarioStore.setState({ revision: initial.revision + 1 }));
  act(() => vi.advanceTimersByTime(400));
  expect(validate).not.toHaveBeenCalled();
  act(() => vi.advanceTimersByTime(100));
  expect(validate).toHaveBeenCalledTimes(1);
  act(() => useScenarioStore.setState({ revision: initial.revision + 2, formErrors: { fragment: 'Incomplete JSON' } }));
  act(() => vi.advanceTimersByTime(1000));
  expect(validate).toHaveBeenCalledTimes(1);
});
it('does not schedule validation for layout-only valid state, View mode, or pending saves', () => {
  const validate = vi.fn();
  useScenarioStore.setState({ mode: 'edit', status: 'valid', validate });
  render(<Validation />);
  act(() => vi.advanceTimersByTime(1000));
  act(() => useScenarioStore.setState({ mode: 'view', status: 'unchecked' }));
  act(() => vi.advanceTimersByTime(1000));
  act(() => useScenarioStore.setState({ mode: 'edit', saving: true }));
  act(() => vi.advanceTimersByTime(1000));
  expect(validate).not.toHaveBeenCalled();
});
