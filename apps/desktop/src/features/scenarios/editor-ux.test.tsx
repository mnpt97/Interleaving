import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createScenarioStore, exampleDocument, useScenarioStore } from './store';
import { useRunStore } from '../runs/store';
import { AddNodePopover } from './AddNodePopover';
import { NodeInspector } from './NodeInspector';
import { UnsavedChangesDialog } from './UnsavedChangesDialog';
const initial = useScenarioStore.getState();
const initialRun = useRunStore.getState();
afterEach(() => {
  useScenarioStore.setState(initial, true);
  useRunStore.setState(initialRun, true);
});
function api() {
  return {
    validate: vi
      .fn()
      .mockImplementation(async (source) => ({ valid: true, document: JSON.parse(source), diagnostics: [] })),
    open: vi.fn().mockResolvedValue(null),
    read: vi.fn(),
    save: vi.fn().mockImplementation(async (source) => ({
      path: '/saved.json',
      validation: { valid: true, document: JSON.parse(source), diagnostics: [] },
    })),
  };
}
describe('workspace mode boundaries', () => {
  it('rejects all persisted mutations in View/Run, including position and history callbacks', () => {
    const bridge = api();
    const store = createScenarioStore(bridge);
    const document = store.getState().document;
    store.getState().command({ type: 'name', name: 'Changed' });
    store.getState().move('health', { x: 999, y: 999 });
    store.setState({
      past: [{ ...exampleDocument, name: 'History' }],
      future: [{ ...exampleDocument, name: 'Future' }],
    });
    store.getState().undo();
    store.getState().redo();
    expect(store.getState().document).toBe(document);
    expect(store.getState().dirty).toBe(false);
    render(<NodeInspector />);
    expect(screen.queryByRole('button', { name: 'Apply position' })).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Path')).not.toBeInTheDocument();
  });
  it('offers cancel/discard/save and returns to the saved baseline', async () => {
    const bridge = api();
    const store = createScenarioStore(bridge);
    await store.getState().setMode('edit');
    store.getState().command({ type: 'name', name: 'Changed' });
    const cancelled = store.getState().setMode('view');
    expect(store.getState().pendingChange).toBe(true);
    await store.getState().resolveChanges('cancel');
    expect(await cancelled).toBe(false);
    expect(store.getState().document?.name).toBe('Changed');
    const discarded = store.getState().setMode('view');
    await store.getState().resolveChanges('discard');
    expect(await discarded).toBe(true);
    expect(store.getState().document).toEqual(exampleDocument);
    await store.getState().setMode('edit');
    store.getState().command({ type: 'name', name: 'Saved name' });
    const saved = store.getState().setMode('view');
    await store.getState().resolveChanges('save');
    expect(await saved).toBe(true);
    expect(store.getState()).toMatchObject({ mode: 'view', dirty: false, savePath: '/saved.json' });
    expect(store.getState().entries.find((e) => e.key === '/saved.json')?.name).toBe('Saved name');
  });
  it('keeps Edit and the draft when saving fails or the destination dialog is cancelled', async () => {
    const bridge = api();
    const store = createScenarioStore(bridge);
    await store.getState().setMode('edit');
    store.getState().command({ type: 'name', name: 'Keep me' });
    bridge.save.mockResolvedValueOnce(false);
    const leaving = store.getState().setMode('view');
    await store.getState().resolveChanges('save');
    expect(store.getState()).toMatchObject({ mode: 'edit', dirty: true, pendingChange: true });
    bridge.save.mockRejectedValueOnce(new Error('write failed'));
    await store.getState().resolveChanges('save');
    expect(store.getState().document?.name).toBe('Keep me');
    await store.getState().resolveChanges('cancel');
    await leaving;
  });
  it('does not lose dirty work when Open is cancelled or the file cannot load', async () => {
    const bridge = api();
    const store = createScenarioStore(bridge);
    await store.getState().setMode('edit');
    store.getState().command({ type: 'name', name: 'Keep me' });
    await store.getState().open();
    expect(store.getState()).toMatchObject({ dirty: true, pendingChange: false });
    bridge.open.mockRejectedValueOnce(new Error('missing file'));
    await store.getState().open();
    expect(store.getState().document?.name).toBe('Keep me');
  });
  it('preserves the original draft if a selected scenario is missing', async () => {
    const bridge = api();
    const store = createScenarioStore(bridge);
    await store.getState().setMode('edit');
    store.getState().command({ type: 'name', name: 'Keep me' });
    store.setState({
      entries: [
        ...store.getState().entries,
        { key: '/missing.json', path: '/missing.json', name: 'Missing', source: '', format: 'json' },
      ],
    });
    bridge.read.mockRejectedValueOnce(new Error('missing'));
    await store.getState().switchScenario('/missing.json');
    expect(store.getState()).toMatchObject({ mode: 'edit', dirty: true, pendingChange: false });
    expect(store.getState().document?.name).toBe('Keep me');
  });
  it('keeps the written baseline and newer edits separate, without concurrent saves', async () => {
    const bridge = api();
    const store = createScenarioStore(bridge);
    let finish!: (value: unknown) => void;
    bridge.save.mockReturnValueOnce(
      new Promise((resolve) => {
        finish = resolve;
      })
    );
    await store.getState().setMode('edit');
    store.getState().command({ type: 'name', name: 'Written' });
    const writing = store.getState().save();
    store.getState().command({ type: 'name', name: 'Newer' });
    expect(await store.getState().save()).toBe(false);
    finish({
      path: '/saved.json',
      validation: { valid: true, document: { ...exampleDocument, name: 'Written' }, diagnostics: [] },
    });
    await writing;
    expect(store.getState()).toMatchObject({ dirty: true, saving: false, savePath: '/saved.json' });
    expect(JSON.parse(store.getState().savedDocument).name).toBe('Written');
    expect(store.getState().document?.name).toBe('Newer');
  });
  it('reuses the JSON destination but keeps YAML source provenance', async () => {
    const bridge = api();
    const store = createScenarioStore(bridge);
    bridge.open.mockResolvedValueOnce({
      path: '/original.yaml',
      format: 'yaml',
      source: 'original YAML',
      validation: { valid: true, document: exampleDocument, diagnostics: [] },
    });
    await store.getState().open();
    await store.getState().setMode('edit');
    store.getState().command({ type: 'name', name: 'First' });
    await store.getState().save();
    expect(bridge.save).toHaveBeenLastCalledWith(expect.any(String), exampleDocument.id, null);
    store.getState().command({ type: 'name', name: 'Second' });
    await store.getState().save();
    expect(bridge.save).toHaveBeenLastCalledWith(expect.any(String), exampleDocument.id, '/saved.json');
    expect(store.getState()).toMatchObject({ path: '/original.yaml', format: 'yaml', originalSource: 'original YAML' });
  });
  it('re-reads selected file entries and blocks switching during an active run', async () => {
    const bridge = api();
    const store = createScenarioStore(bridge);
    const opened = {
      path: '/scenario.json',
      format: 'json',
      source: JSON.stringify(exampleDocument),
      validation: { valid: true, document: exampleDocument, diagnostics: [] },
    };
    bridge.open.mockResolvedValueOnce(opened);
    await store.getState().open();
    bridge.read.mockResolvedValueOnce({
      ...opened,
      validation: { ...opened.validation, document: { ...exampleDocument, name: 'External edit' } },
    });
    await store.getState().switchScenario('/scenario.json');
    expect(store.getState().document?.name).toBe('External edit');
    useRunStore.setState({ status: 'running' });
    await store.getState().switchScenario(store.getState().entries[0].key);
    expect(store.getState().document?.name).toBe('External edit');
  });
});

describe('HTTP authoring surface', () => {
  it('adds HTTP Request through the icon picker, configures it, and undoes the whole insertion', async () => {
    const user = userEvent.setup();
    await useScenarioStore.getState().newScenario();
    render(
      <>
        <AddNodePopover />
        <NodeInspector />
      </>
    );
    const before = structuredClone(useScenarioStore.getState().document);
    await user.click(screen.getByRole('button', { name: 'Add node' }));
    expect(screen.getByRole('group', { name: 'Add node types' })).toBeInTheDocument();
    for (const name of ['HTTP Request', 'Extract', 'Set session', 'Call workflow'])
      expect(screen.getByRole('button', { name })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /HTTP Request/ }));
    expect(screen.queryByRole('group', { name: 'Add node types' })).not.toBeInTheDocument();
    const workflow = useScenarioStore.getState().document!.workflows[0];
    expect(workflow.nodes.map((n) => n.type)).toEqual(['start', 'end', 'http']);
    expect(workflow.edges).toEqual([]);
    const path = screen.getByRole('textbox', { name: 'Path' });
    await user.type(path, '/health');
    await user.tab();
    expect(useScenarioStore.getState().formErrors).toEqual({});
    expect(useScenarioStore.getState().document!.workflows[0].nodes.find((node) => node.id === 'http-1')).toMatchObject(
      {
        config: { path: '/health', method: 'GET' },
      }
    );
    act(() => {
      useScenarioStore.getState().undo();
      useScenarioStore.getState().undo();
    });
    expect(useScenarioStore.getState().document).toEqual(before);
  });
  it('edits a node display name without changing its stable ID or connections', async () => {
    const user = userEvent.setup();
    await useScenarioStore.getState().newScenario();
    act(() => useScenarioStore.getState().command({ type: 'node.add', kind: 'http', workflowId: 'workflow-1' }));
    render(<NodeInspector />);
    const before = structuredClone(useScenarioStore.getState().document!.workflows[0]);
    const input = screen.getByRole('textbox', { name: 'Node name' });
    await user.clear(input);
    await user.type(input, 'Get inventory');
    await user.click(screen.getByRole('button', { name: 'Apply configuration' }));
    const workflow = useScenarioStore.getState().document!.workflows[0];
    expect(workflow.nodes.find((node) => node.id === 'http-1')).toMatchObject({ id: 'http-1', name: 'Get inventory' });
    expect(workflow.edges).toEqual(before.edges);
    expect(screen.getByRole('heading', { name: 'Get inventory' })).toBeInTheDocument();
    act(() => useScenarioStore.getState().undo());
    expect(input).toHaveValue('HTTP Request');
    act(() => useScenarioStore.getState().redo());
    expect(input).toHaveValue('Get inventory');
  });
  it('supports Escape and focus restoration for the inline picker', async () => {
    const user = userEvent.setup();
    await useScenarioStore.getState().setMode('edit');
    render(<AddNodePopover />);
    const trigger = screen.getByRole('button', { name: 'Add node' });
    await user.click(trigger);
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('group', { name: 'Add node types' })).not.toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Add node' })).toHaveFocus());
  });
  it.each(['escape', 'outside', 'close'])(
    'keeps editing when the unsaved dialog is dismissed by %s',
    async (dismissal) => {
      const user = userEvent.setup();
      await useScenarioStore.getState().setMode('edit');
      act(() => useScenarioStore.getState().command({ type: 'name', name: 'Keep this' }));
      render(<UnsavedChangesDialog />);
      let leaving!: Promise<boolean>;
      act(() => {
        leaving = useScenarioStore.getState().setMode('view');
      });
      expect(screen.getByRole('dialog', { name: 'Save your changes?' })).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Cancel' })).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Discard' })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Save' })).toBeInTheDocument();
      if (dismissal === 'escape') await user.keyboard('{Escape}');
      if (dismissal === 'close') await user.click(screen.getByRole('button', { name: 'Close' }));
      if (dismissal === 'outside') await user.click(document.querySelector('.dialog-overlay')!);
      expect(await leaving).toBe(false);
      expect(useScenarioStore.getState().document?.name).toBe('Keep this');
    }
  );
});
