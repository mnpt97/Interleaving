import { emptyEventContext } from '../../lib/run-types';
import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { RunPanel } from './RunPanel';
import { useRunStore } from './store';
import { useScenarioStore } from '../scenarios/store';
vi.mock('../../lib/desktop', async (original) => ({
  ...(await original<typeof import('../../lib/desktop')>()),
  desktopAvailable: true,
}));
const initialScenario = useScenarioStore.getState();
const initial = useRunStore.getState();
afterEach(() => {
  useRunStore.setState(initial, true);
  useScenarioStore.setState(initialScenario, true);
});
function openEvent(type: string) {
  const summary = screen.queryAllByRole('button', { expanded: false })[0];
  if (summary) fireEvent.click(summary);
  fireEvent.click(screen.getByRole('button', { name: new RegExp(type.replace(/\./g, '\\.')) }));
}
describe('HTTP response inspector', () => {
  it.each(['error', 'failed'] as const)('allows retry after %s and a layout change', async (status) => {
    const start = vi.fn();
    useScenarioStore.setState({ status: 'valid', mode: 'edit' });
    useRunStore.setState({ status, start });
    useScenarioStore.getState().move('health', { x: 999, y: 99 });
    render(<RunPanel />);
    const retry = screen.getByRole('button', { name: 'Run' });
    expect(retry).toBeEnabled();
    fireEvent.click(retry);
    await useScenarioStore.getState().resolveChanges('discard');
    await vi.waitFor(() => expect(start).toHaveBeenCalledWith(useScenarioStore.getState().document));
  });

  it('shows HTTP 404 as a received response and displays redacted details', () => {
    useRunStore.setState({
      status: 'passed',
      events: [
        {
          runId: 'run',
          sequence: 1,
          context: emptyEventContext,
          elapsedNs: 1000000,
          userId: 'user',
          nodeId: 'health',
          type: 'response.received',
          payload: {
            status: 404,
            headers: { 'set-cookie': '[REDACTED]' },
            body: { value: { error: 'not found' }, bytes: 10, truncated: false, omitted: false },
          },
        },
      ],
    });
    render(<RunPanel />);
    openEvent('response.received');
    expect(screen.getByRole('heading', { name: 'response.received · HTTP 404' })).toBeInTheDocument();
    expect(screen.getByLabelText('Request and response inspector')).toHaveTextContent('[REDACTED]');
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
  it('shows transport failures clearly', () => {
    useRunStore.setState({ status: 'error', error: 'timeout: Request timed out' });
    render(<RunPanel />);
    expect(screen.getByRole('alert')).toHaveTextContent('timeout: Request timed out');
  });
});

it('shows session changes as names only', () => {
  useRunStore.setState({
    status: 'passed',
    events: [
      {
        runId: 'auth',
        sequence: 1,
        context: emptyEventContext,
        elapsedNs: 1,
        userId: 'user:1',
        nodeId: 'apply-token',
        type: 'session.updated',
        payload: { setHeaders: ['authorization'], removedHeaders: ['x-old'] },
      },
    ],
  });
  render(<RunPanel />);
  openEvent('session.updated');
  expect(screen.getByLabelText('Session changes')).toHaveTextContent('authorization');
  expect(screen.getByLabelText('Session changes')).toHaveTextContent('x-old');
});

it('explains authentication payload omission while retaining HTTP status', () => {
  useRunStore.setState({
    status: 'passed',
    events: [
      {
        runId: 'auth',
        sequence: 1,
        context: emptyEventContext,
        elapsedNs: 1,
        userId: 'user:1',
        nodeId: 'login',
        type: 'response.received',
        payload: {
          status: 200,
          headers: { '[omitted]': 'Sensitive authentication header values omitted' },
          body: {
            value: null,
            bytes: 300,
            truncated: false,
            omitted: true,
            reason: 'Sensitive authentication payload omitted',
          },
        },
      },
    ],
  });
  render(<RunPanel />);
  openEvent('response.received');
  expect(screen.getByRole('heading', { name: /HTTP 200/ })).toBeInTheDocument();
  expect(screen.getByText(/Authentication payload hidden/)).toBeInTheDocument();
});

it('shows safe extraction failures from the engine', () => {
  useRunStore.setState({
    status: 'error',
    events: [
      {
        runId: 'auth',
        sequence: 1,
        context: emptyEventContext,
        elapsedNs: 1,
        userId: 'user:1',
        nodeId: 'extract-token',
        type: 'step.failed',
        payload: { error: { kind: 'extractionMissing', message: 'Extraction property is missing' } },
      },
    ],
  });
  render(<RunPanel />);
  openEvent('step.failed');
  expect(screen.getByText('extractionMissing: Extraction property is missing')).toBeInTheDocument();
});
