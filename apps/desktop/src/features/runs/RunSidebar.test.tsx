import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { RunSidebar } from './RunSidebar';
import { useRunStore } from './store';
import { useScenarioStore } from '../scenarios/store';
import { begin, observation } from './fixtures';

const original = useRunStore.getState();
const scenario = useScenarioStore.getState();
afterEach(() => {
  cleanup();
  useRunStore.setState(original, true);
  useScenarioStore.setState(scenario, true);
});

it('shows captured steps under Iteration by default and selects their execution scope', async () => {
  useScenarioStore.setState({ mode: 'view' });
  useRunStore.setState({
    canvasView: 'lifecycle',
    selectedWorkflow: null,
    selectedInvocation: null,
    selectedNode: null,
    report: null,
    status: 'running',
    showRun: true,
    events: [begin(), observation('workflow.started', 2, {}, { nodeKey: null }, null)],
  });
  render(<RunSidebar />);
  expect(screen.getByRole('button', { name: /^Iteration / })).toHaveAttribute('aria-expanded', 'true');
  expect(screen.getByRole('button', { name: /^Cleanup / })).toHaveAttribute('aria-expanded', 'false');
  const health = screen.getByRole('button', { name: /Check health/ });
  expect(health).not.toHaveTextContent(/pending/i);
  await userEvent.hover(within(health).getByRole('img', { name: 'Pending' }));
  expect(await screen.findByRole('tooltip')).toHaveTextContent('Pending');
  await userEvent.click(screen.getByRole('button', { name: /Check health/ }));
  expect(useRunStore.getState().selectedNode).toBe('h');
  expect(useRunStore.getState().selectedInvocation).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: /^Cleanup / }));
  expect(screen.getByRole('button', { name: /Child health/ })).toBeDisabled();
});
