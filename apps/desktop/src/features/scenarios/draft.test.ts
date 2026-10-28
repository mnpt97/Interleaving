import { describe, expect, it, vi } from 'vitest';
import { allocateId, executeCommand, newDraft } from './draft';
import { createScenarioStore, exampleDocument } from './store';
const bridge = () => ({
  validate: vi.fn().mockResolvedValue({ valid: false, document: null, diagnostics: [] }),
  open: vi.fn().mockResolvedValue(null),
  save: vi.fn().mockResolvedValue(true),
});
describe('draft lifecycle', () => {
  it('creates typed skeletons and allocates collision-free scoped IDs', () => {
    const draft = newDraft();
    expect(draft.workflows[0].edges[0]).toMatchObject({ from: 'start', to: 'end' });
    expect(newDraft().id).not.toBe(draft.id);
    expect(allocateId('call', ['call-1', 'call-3'])).toBe('call-2');
    const copy = executeCommand(draft, { type: 'workflow.duplicate', workflowId: 'workflow-1' });
    expect(copy.workflows[1].id).toBe('workflow-2');
    expect(copy.workflows[1].nodes).toEqual(draft.workflows[0].nodes);
    expect(() => executeCommand(draft, { type: 'workflow.delete', workflowId: 'workflow-1' })).toThrow('referenced');
  });
  it('undoes coalesced text and drag changes and returns to the saved baseline', () => {
    const store = createScenarioStore(bridge());
    store.setState({ mode: 'edit' });
    store.getState().command({ type: 'name', name: 'A' }, 'name');
    store.getState().command({ type: 'name', name: 'AB' }, 'name');
    expect(store.getState().past).toHaveLength(1);
    store.getState().undo();
    expect(store.getState().document).toEqual(exampleDocument);
    expect(store.getState().dirty).toBe(false);
    store.getState().redo();
    expect(store.getState().document?.name).toBe('AB');
    store.getState().move('health', { x: 1, y: 2 });
    store.getState().move('health', { x: 3, y: 4 });
    store.getState().endHistoryGroup();
    store.getState().move('health', { x: 5, y: 6 });
    store.getState().undo();
    expect(store.getState().document?.workflows[0].layout?.positions.health).toEqual({ x: 3, y: 4 });
  });
  it('guards replacement and retains work after cancelled open/save', async () => {
    const api = bridge();
    const confirm = vi.fn().mockReturnValue(false);
    const store = createScenarioStore(api, true, confirm);
    store.setState({ mode: 'edit' });
    store.getState().command({ type: 'name', name: 'Draft' });
    await store.getState().open();
    expect(store.getState().document?.name).toBe('Draft');
    await store.getState().newScenario();
    expect(store.getState().document?.name).toBe('Draft');
    confirm.mockReturnValue(true);
    await store.getState().open();
    expect(store.getState().dirty).toBe(true);
    api.save.mockResolvedValue(false);
    await store.getState().save();
    expect(store.getState().dirty).toBe(true);
  });
  it('retains invalid drafts and keeps form fragments out of persistence', async () => {
    const api = bridge();
    const store = createScenarioStore(api);
    store.setState({ mode: 'edit' });
    await store.getState().newScenario();
    const draft = store.getState().document;
    store.getState().setFormError('timeout', 'Enter a number');
    await store.getState().validate();
    expect(store.getState().document).toBe(draft);
    expect(store.getState().status).toBe('invalid');
    expect(await store.getState().save()).toBe(false);
    expect(api.save).not.toHaveBeenCalled();
    store.getState().setFormError('timeout', null);
    await store.getState().save();
    expect(JSON.parse(api.save.mock.calls[0][0])).toEqual(draft);
  });
  it('scopes movement to the selected workflow and preserves accepted run input', () => {
    const store = createScenarioStore(bridge());
    store.setState({ mode: 'edit' });
    store.getState().command({ type: 'workflow.duplicate', workflowId: exampleDocument.workflows[0].id });
    const captured = JSON.stringify(store.getState().document);
    const id = store.getState().document!.workflows[1].id;
    store.getState().selectWorkflow(id);
    store.getState().move('health', { x: 1234, y: 4321 });
    expect(store.getState().document!.workflows[0]).toEqual(JSON.parse(captured).workflows[0]);
    expect(store.getState().document!.workflows[1].layout?.positions.health.x).toBe(1234);
    expect(JSON.parse(captured).workflows[1].layout?.positions.health.x).not.toBe(1234);
  });
  it('does not mark newer edits saved when an older save completes', async () => {
    let resolve!: (value: boolean) => void;
    const api = bridge();
    api.save.mockReturnValue(
      new Promise<boolean>((done) => {
        resolve = done;
      })
    );
    const store = createScenarioStore(api);
    store.setState({ mode: 'edit' });
    store.getState().command({ type: 'name', name: 'First' });
    store.getState().command({ type: 'name', name: 'Save changes' });
    const saving = store.getState().save();
    store.getState().command({ type: 'name', name: 'Second' });
    resolve(true);
    await saving;
    expect(store.getState().dirty).toBe(true);
    expect(store.getState().document?.name).toBe('Second');
  });
  it('rejects deletion of a referenced node including escaped identifiers', () => {
    const draft = newDraft();
    draft.workflows[0].nodes.push(
      { id: 'request"1', type: 'http', config: { method: 'GET', path: '/', headers: {} } },
      {
        id: 'extract',
        type: 'extract',
        config: { sourceNodeId: 'request"1', expression: 'response.body.token', output: 'token', type: 'string' },
      }
    );
    expect(() => executeCommand(draft, { type: 'node.delete', workflowId: 'workflow-1', nodeId: 'request"1' })).toThrow(
      'referenced'
    );
  });
});
