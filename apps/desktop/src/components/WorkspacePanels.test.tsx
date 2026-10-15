import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from '../App';
import { WorkspacePanels } from './WorkspacePanels';
import { useScenarioStore } from '../features/scenarios/store';
import { useRunStore } from '../features/runs/store';

vi.mock('../lib/tauri', async (original) => ({
  ...(await original<typeof import('../lib/tauri')>()),
  desktopAvailable: true,
}));
vi.mock('./ThemeControl', () => ({ ThemeControl: () => null }));
vi.mock('../features/scenarios/ScenarioCanvas', () => ({ ScenarioCanvas: () => <div>Workflow canvas</div> }));
const initialScenario = useScenarioStore.getState();
const initialRun = useRunStore.getState();
beforeEach(() => {
  useScenarioStore.setState(
    {
      ...initialScenario,
      status: 'valid',
      loadExample: vi.fn().mockResolvedValue(undefined),
      validate: vi.fn().mockResolvedValue(undefined),
    },
    true
  );
  useRunStore.setState(initialRun, true);
});
afterEach(() => {
  useScenarioStore.setState(initialScenario, true);
  useRunStore.setState(initialRun, true);
});

describe('workspace panel tabs', () => {
  it('toggles the inspector and reopens it when selecting the same or another node', async () => {
    const user = userEvent.setup();
    render(<App />);
    const toggle = screen.getByRole('button', { name: 'Inspector' });
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    await user.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByRole('complementary', { name: 'Node inspector' })).not.toBeInTheDocument();
    expect(document.querySelector('.workspace')).toHaveClass('inspector-collapsed');
    const nodes = screen.getByRole('navigation', { name: 'Workflow nodes' });
    await user.click(within(nodes).getByRole('button', { name: /health/ }));
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByRole('complementary', { name: 'Node inspector' })).toBeInTheDocument();
    await user.click(toggle);
    await user.click(within(nodes).getByRole('button', { name: /start/ }));
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    await user.click(toggle);
    await user.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
  });

  it('shows Play by default and after successful or cancelled runs', () => {
    render(<WorkspacePanels />);
    const run = screen.getByRole('tab', { name: 'Run' });
    expect(run).toHaveAccessibleDescription('Ready to run');
    expect(run.querySelector('.run-tab-play')).toBeInTheDocument();
    for (const status of ['passed', 'cancelled'] as const) {
      act(() => useRunStore.setState({ status }));
      expect(run.querySelector('.run-tab-play')).toBeInTheDocument();
      expect(run.querySelector('.run-tab-warning')).not.toBeInTheDocument();
    }
  });

  it('shows loading throughout active execution, warnings on failure, and resets for a new run', () => {
    render(<WorkspacePanels />);
    const run = screen.getByRole('tab', { name: 'Run' });
    for (const status of ['starting', 'running', 'cancelling'] as const) {
      act(() => useRunStore.setState({ status }));
      expect(run.querySelector('.run-tab-loading')).toHaveClass('spin');
      expect(run.querySelector('.run-tab-play')).not.toBeInTheDocument();
    }
    for (const status of ['failed', 'error'] as const) {
      act(() => useRunStore.setState({ status }));
      expect(run).toHaveAccessibleDescription('Execution failed or returned HTTP errors');
      expect(run.querySelector('.run-tab-warning')).toBeInTheDocument();
      expect(run.querySelector('.run-tab-loading')).not.toBeInTheDocument();
    }
    act(() => useRunStore.setState({ status: 'starting', report: null, events: [] }));
    expect(run.querySelector('.run-tab-loading')).toBeInTheDocument();
    expect(run.querySelector('.run-tab-warning')).not.toBeInTheDocument();
  });

  it('warns for HTTP errors even if the workflow completed and the report omitted response events', () => {
    useRunStore.setState({
      status: 'passed',
      report: {
        reportVersion: 2,
        runId: 'done',
        status: 'passed',
        snapshot: {},
        summary: { requests: 2, responses: 2, httpErrorResponses: 1, elapsedNs: 10 },
        events: [],
        error: null,
        eventsTruncated: true,
        deliveryDroppedEvents: 0,
      },
    });
    render(<WorkspacePanels />);
    const run = screen.getByRole('tab', { name: 'Run' });
    expect(run.querySelector('.run-tab-warning')).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Validation' })).toHaveAttribute('aria-selected', 'true');
  });

  it("places Validate inside Validation and switches to Run's actions", async () => {
    const user = userEvent.setup();
    render(<App />);
    expect(
      within(screen.getByRole('region', { name: 'Scenario actions' })).queryByRole('button', { name: 'Validate' })
    ).not.toBeInTheDocument();
    const validation = screen.getByRole('tabpanel', { name: 'Validation' });
    await user.click(within(validation).getByRole('button', { name: 'Validate' }));
    expect(useScenarioStore.getState().validate).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole('tab', { name: 'Run' }));
    expect(screen.getAllByRole('tabpanel')).toHaveLength(1);
    expect(screen.queryByRole('button', { name: 'Validate' })).not.toBeInTheDocument();
    expect(within(screen.getByRole('tabpanel', { name: 'Run' })).getByRole('button', { name: 'Run' })).toBeEnabled();
  });

  it('supports arrow-key navigation and retains run events and selection across tabs', async () => {
    const user = userEvent.setup();
    useRunStore.setState({
      status: 'running',
      runId: 'active',
      selected: 1,
      events: [
        {
          runId: 'active',
          sequence: 1,
          elapsedNs: 1000,
          userId: 'user',
          nodeId: 'health',
          type: 'response.received',
          payload: {
            status: 200,
            headers: {},
            body: { value: { status: 'ok' }, bytes: 15, truncated: false, omitted: false },
          },
        },
      ],
    });
    render(<WorkspacePanels />);
    screen.getByRole('tab', { name: 'Validation' }).focus();
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('tab', { name: 'Run' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeEnabled();
    expect(screen.getByRole('heading', { name: 'response.received · HTTP 200' })).toBeInTheDocument();
    await user.keyboard('{ArrowLeft}');
    expect(screen.getByRole('tab', { name: 'Validation' })).toHaveAttribute('aria-selected', 'true');
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('heading', { name: 'response.received · HTTP 200' })).toBeInTheDocument();
    expect(useRunStore.getState().status).toBe('running');
    expect(useRunStore.getState().events).toHaveLength(1);
  });

  it('keeps Validate disabled during pending work', () => {
    useScenarioStore.setState({ busy: true });
    render(<WorkspacePanels />);
    expect(screen.getByRole('button', { name: 'Validate' })).toBeDisabled();
  });

  it('shows a warning and opens Validation on failed loads, including consecutive invalid files', async () => {
    const user = userEvent.setup();
    render(<WorkspacePanels />);
    await user.click(screen.getByRole('tab', { name: 'Run' }));
    act(() => useScenarioStore.setState({ status: 'invalid', fileRevision: 1 }));
    const validation = screen.getByRole('tab', { name: 'Validation' });
    expect(validation).toHaveAttribute('aria-selected', 'true');
    expect(validation).toHaveAccessibleDescription('Validation failed');
    expect(validation.querySelector('.validation-tab-warning')).toBeInTheDocument();
    expect(validation.querySelector('.validation-tab-check')).not.toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Run' }));
    expect(screen.getByRole('tab', { name: 'Run' })).toHaveAttribute('aria-selected', 'true');
    act(() => useScenarioStore.setState({ busy: true }));
    expect(screen.getByRole('tab', { name: 'Run' })).toHaveAttribute('aria-selected', 'true');
    act(() => useScenarioStore.setState({ busy: false, status: 'invalid', fileRevision: 2 }));
    expect(validation).toHaveAttribute('aria-selected', 'true');
  });

  it('shows a check for valid loads without changing the selected tab, including after a failure', async () => {
    const user = userEvent.setup();
    render(<WorkspacePanels />);
    await user.click(screen.getByRole('tab', { name: 'Run' }));
    act(() => useScenarioStore.setState({ status: 'valid', fileRevision: 1 }));
    const validation = screen.getByRole('tab', { name: 'Validation' });
    expect(validation).toHaveAccessibleDescription('Validation passed');
    expect(validation.querySelector('.validation-tab-check')).toHaveAttribute('stroke', 'currentColor');
    expect(screen.getByRole('tab', { name: 'Run' })).toHaveAttribute('aria-selected', 'true');
    act(() => useScenarioStore.setState({ status: 'invalid', fileRevision: 2 }));
    await user.click(screen.getByRole('tab', { name: 'Run' }));
    act(() => useScenarioStore.setState({ status: 'valid', fileRevision: 3 }));
    expect(validation.querySelector('.validation-tab-warning')).not.toBeInTheDocument();
    expect(validation.querySelector('.validation-tab-check')).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Run' })).toHaveAttribute('aria-selected', 'true');
  });

  it('shows no result icon before validation', () => {
    useScenarioStore.setState({ status: 'unchecked' });
    render(<WorkspacePanels />);
    const validation = screen.getByRole('tab', { name: 'Validation' });
    expect(validation).toHaveAccessibleDescription('Not validated');
    expect(validation.querySelector('svg')).not.toBeInTheDocument();
  });

  it('shows a rotating loader during validation, then restores the result icon', () => {
    useScenarioStore.setState({ busy: true, validating: true });
    render(<WorkspacePanels />);
    const validation = screen.getByRole('tab', { name: 'Validation' });
    expect(validation).toHaveAccessibleDescription('Validating scenario');
    expect(validation.querySelector('.validation-tab-loading')).toHaveClass('spin');
    expect(validation.querySelector('.validation-tab-check')).not.toBeInTheDocument();
    act(() => useScenarioStore.setState({ busy: false, validating: false, status: 'valid' }));
    expect(validation.querySelector('.validation-tab-loading')).not.toBeInTheDocument();
    expect(validation.querySelector('.validation-tab-check')).toBeInTheDocument();
  });

  it('does not show a validation loader for unrelated busy work', () => {
    useScenarioStore.setState({ busy: true, validating: false });
    render(<WorkspacePanels />);
    const validation = screen.getByRole('tab', { name: 'Validation' });
    expect(validation.querySelector('.validation-tab-loading')).not.toBeInTheDocument();
    expect(validation.querySelector('.validation-tab-check')).toBeInTheDocument();
  });
});
