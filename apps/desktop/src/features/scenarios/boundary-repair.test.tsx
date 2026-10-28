import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { newDraft, executeCommand } from './draft';
import { AddNodePopover } from './AddNodePopover';
import { useScenarioStore } from './store';
const initial = useScenarioStore.getState();
afterEach(() => useScenarioStore.setState(initial, true));

it('repairs missing boundaries without fabricating connections and restores the empty draft on undo', async () => {
  const user = userEvent.setup();
  const document = newDraft();
  document.workflows[0].nodes = [];
  document.workflows[0].edges = [];
  document.workflows[0].layout = { positions: {} };
  useScenarioStore.setState({
    mode: 'edit',
    document,
    selectedWorkflowId: 'workflow-1',
    selectedId: null,
    past: [],
    future: [],
  });
  render(<AddNodePopover />);
  await user.click(screen.getByRole('button', { name: 'Add node' }));
  await user.click(screen.getByRole('button', { name: 'Start' }));
  expect(useScenarioStore.getState().document!.workflows[0].nodes[0].type).toBe('start');
  await user.click(screen.getByRole('button', { name: 'Add node' }));
  expect(screen.queryByRole('button', { name: 'Start' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'End' }));
  expect(useScenarioStore.getState().document!.workflows[0].nodes.map((node) => node.type)).toEqual(['start', 'end']);
  expect(useScenarioStore.getState().document!.workflows[0].edges).toEqual([]);
  act(() => {
    useScenarioStore.getState().undo();
    useScenarioStore.getState().undo();
  });
  expect(useScenarioStore.getState().document).toEqual(document);
});

it('rejects duplicate boundaries and ambiguous automatic insertion', () => {
  const document = newDraft();
  expect(() => executeCommand(document, { type: 'boundary.add', workflowId: 'workflow-1', kind: 'start' })).toThrow(
    'already has'
  );
  document.workflows[0].nodes.push({ id: 'other', type: 'end' });
  document.workflows[0].edges.push({ id: 'other-edge', from: 'start', to: 'other' });
  expect(() => executeCommand(document, { type: 'http.insert', workflowId: 'workflow-1' })).toThrow('ambiguous');
});
