import { act, cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { LifecycleSidebar } from './LifecycleSidebar';
import { useScenarioStore } from './store';
import { executeCommand, newDraft } from './draft';
import { phases, phaseWorkflow, phaseName } from './lifecycle';

const original = useScenarioStore.getState();
afterEach(() => {
  cleanup();
  useScenarioStore.setState(original, true);
});

function setup() {
  let document = newDraft();
  for (const phase of phases) {
    document = executeCommand(document, { type: 'phase.node.add', phase, kind: 'http' });
    phaseWorkflow(document, phase)!.nodes.find((node) => node.type === 'http')!.name = `${phaseName[phase]} request`;
  }
  useScenarioStore.setState({
    document,
    selectedPhase: 'iteration',
    selectedId: null,
    formErrors: {},
    selectedWorkflowId: phaseWorkflow(document, 'iteration')!.id,
    canvasView: 'lifecycle',
  });
  render(<LifecycleSidebar />);
}

it('opens Iteration by default and lets other phases expand independently', async () => {
  setup();
  expect(screen.getByRole('button', { name: 'Iteration' })).toHaveAttribute('aria-expanded', 'true');
  expect(screen.getByRole('button', { name: 'Initialize' })).toHaveAttribute('aria-expanded', 'false');
  expect(screen.getByRole('button', { name: 'Cleanup' })).toHaveAttribute('aria-expanded', 'false');
  expect(screen.getByRole('button', { name: /Iteration request/ })).toBeVisible();
  expect(screen.queryByRole('button', { name: /Initialize request/ })).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole('button', { name: 'Initialize' }));
  expect(screen.getByRole('button', { name: /Initialize request/ })).toBeVisible();
  expect(screen.getByRole('button', { name: /Iteration request/ })).toBeVisible();
  await userEvent.click(screen.getByRole('button', { name: /Initialize request/ }));
  expect(useScenarioStore.getState()).toMatchObject({ selectedPhase: 'initialize', selectedId: 'http-1' });
});

it('supports keyboard disclosure and exposes selections made on the canvas', async () => {
  setup();
  const initialize = screen.getByRole('button', { name: 'Initialize' });
  initialize.focus();
  await userEvent.keyboard('{ArrowDown}');
  expect(screen.getByRole('button', { name: 'Iteration' })).toHaveFocus();
  await userEvent.keyboard('{Enter}');
  expect(screen.queryByRole('button', { name: /Iteration request/ })).not.toBeInTheDocument();
  act(() => useScenarioStore.getState().selectPhase('cleanup', 'http-1'));
  expect(screen.getByRole('button', { name: /Cleanup request/ })).toHaveAttribute('aria-current', 'true');
  expect(screen.getByRole('button', { name: 'Cleanup' })).toHaveAttribute('aria-expanded', 'true');
});
