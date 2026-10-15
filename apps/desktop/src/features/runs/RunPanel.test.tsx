import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { RunPanel } from './RunPanel';
import { useRunStore } from './store';
const initial = useRunStore.getState();
afterEach(() => useRunStore.setState(initial, true));
describe('HTTP response inspector', () => {
  it('shows HTTP 404 as a received response and displays redacted details', () => {
    useRunStore.setState({
      status: 'passed',
      events: [
        {
          runId: 'run',
          sequence: 1,
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
    fireEvent.click(screen.getByRole('button', { name: /response.received/ }));
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
        elapsedNs: 1,
        userId: 'user:1',
        nodeId: 'apply-token',
        type: 'session.updated',
        payload: { setHeaders: ['authorization'], removedHeaders: ['x-old'] },
      },
    ],
  });
  render(<RunPanel />);
  fireEvent.click(screen.getByRole('button', { name: /session.updated/ }));
  expect(screen.getByLabelText('Session changes')).toHaveTextContent('authorization');
  expect(screen.getByLabelText('Session changes')).toHaveTextContent('x-old');
  expect(screen.getByLabelText('Session changes')).toHaveTextContent('Values stay in Rust');
});

it('explains authentication payload omission while retaining HTTP status', () => {
  useRunStore.setState({
    status: 'passed',
    events: [
      {
        runId: 'auth',
        sequence: 1,
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
  fireEvent.click(screen.getByRole('button', { name: /response.received/ }));
  expect(screen.getByRole('heading', { name: /HTTP 200/ })).toBeInTheDocument();
  expect(screen.getByText(/Authentication capture hides sensitive/)).toBeInTheDocument();
});

it('shows safe extraction failures from the engine', () => {
  useRunStore.setState({
    status: 'error',
    events: [
      {
        runId: 'auth',
        sequence: 1,
        elapsedNs: 1,
        userId: 'user:1',
        nodeId: 'extract-token',
        type: 'step.failed',
        payload: { error: { kind: 'extractionMissing', message: 'Extraction property is missing' } },
      },
    ],
  });
  render(<RunPanel />);
  fireEvent.click(screen.getByRole('button', { name: /step.failed/ }));
  expect(screen.getByText('extractionMissing: Extraction property is missing')).toBeInTheDocument();
});
