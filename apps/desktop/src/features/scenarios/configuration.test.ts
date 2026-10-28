import { describe, expect, it, vi } from 'vitest';
import { executeCommand, newDraft } from './draft';
import { configureScenario, configureWorkflow, outputConsumers, type ScenarioConfiguration } from './configuration';
import { createScenarioStore } from './store';
function configuration(document = newDraft()): ScenarioConfiguration {
  return {
    environment: document.environment,
    clientProfiles: document.clientProfiles,
    groups: document.groups,
    identities: document.identities,
  };
}
function request() {
  const document = executeCommand(newDraft(), { type: 'http.insert', workflowId: 'workflow-1' });
  const node = document.workflows[0].nodes[1];
  if (node.type !== 'http') throw new Error('HTTP');
  node.config.path = '/health';
  return document;
}

describe('configuration safety', () => {
  it('protects scenario-variable references and path interpolation but ignores literal JSON descriptors', () => {
    const document = request();
    document.environment.variables.id = '1';
    const node = document.workflows[0].nodes[1];
    if (node.type !== 'http') throw new Error('HTTP');
    node.config.path = '/items/${id}';
    const config = { ...configuration(document), environment: { variables: {} } };
    expect(() => configureScenario(document, config)).toThrow('request path');
    node.config.path = '/health';
    node.config.body = { kind: 'jsonTemplate', fields: { id: { source: 'environment', name: 'id' } } };
    expect(() => configureScenario(document, config)).toThrow('referenced');
    node.config.body = { kind: 'jsonTemplate', fields: { default: { source: 'environment', name: 'id' } } };
    expect(() => configureScenario(document, config)).toThrow('referenced');
    node.config.body = { kind: 'json', value: { source: 'environment', name: 'id' } };
    document.environment.variables.payload = { source: 'environment', name: 'id' };
    expect(() => configureScenario(document, config)).not.toThrow();
  });
  it('protects assigned profiles and identity references without changing native Authentication ownership', () => {
    const document = executeCommand(newDraft(), { type: 'authentication.insert', workflowId: 'workflow-1' });
    expect(() => configureScenario(document, { ...configuration(document), clientProfiles: [] })).toThrow('profile');
    const existingInvalid = structuredClone(document);
    existingInvalid.groups[0].clientProfileId = 'missing';
    expect(() => configureScenario(existingInvalid, configuration(existingInvalid))).not.toThrow();
    const before = structuredClone(document.workflows);
    const changed = configureScenario(document, {
      ...configuration(document),
      identities: [{ id: 'optional', credentials: { username: { source: 'env', name: 'USER_NAME' } } }],
    });
    expect(changed.workflows).toEqual(before);
    expect(JSON.stringify(changed)).not.toContain('credentialValue');
  });
  it('rejects referenced input renames and output removal across calls and lifecycle mappings', () => {
    let document = executeCommand(request(), {
      type: 'node.insert',
      workflowId: 'workflow-1',
      kind: 'call',
      newWorkflowName: 'Child',
    });
    const child = document.workflows[1];
    child.inputs.name = { type: 'string' };
    child.outputs.name = { type: 'string', value: { source: 'input', name: 'name' } };
    expect(() =>
      configureWorkflow(document, child.id, { name: child.name, inputs: {}, outputs: child.outputs })
    ).toThrow('Input name');
    const parent = document.workflows[0];
    parent.outputs.result = { type: 'string', value: { source: 'callOutput', nodeId: 'call-1', name: 'name' } };
    expect(() =>
      configureWorkflow(document, child.id, { name: child.name, inputs: child.inputs, outputs: {} })
    ).toThrow('consumers');
    expect(outputConsumers(document, child.id, 'name')).toEqual(['Iteration outputs']);
    delete parent.outputs.result;
    document.groups[0].lifecycle.initialize = { workflowId: child.id, inputs: {} };
    document.groups[0].lifecycle.iteration.inputs.name = { source: 'lifecycle', phase: 'initialize', name: 'name' };
    expect(() =>
      configureWorkflow(document, child.id, { name: child.name, inputs: child.inputs, outputs: {} })
    ).toThrow('initialize output');
  });
  it('applies metadata atomically without replacing newer graph layout and supports undo/redo', () => {
    const store = createScenarioStore({ validate: vi.fn(), open: vi.fn(), save: vi.fn() });
    const document = request();
    store.setState({ document, selectedWorkflowId: 'workflow-1', mode: 'edit' });
    store.getState().move('http-1', { x: 777, y: 200 });
    store.getState().command({
      type: 'workflow.configure',
      workflowId: 'workflow-1',
      configuration: { name: 'Renamed', inputs: {}, outputs: {} },
    });
    expect(store.getState().document!.workflows[0].layout!.positions['http-1'].x).toBe(777);
    store.getState().undo();
    expect(store.getState().document!.workflows[0].name).toBe('Iteration');
    store.getState().redo();
    expect(store.getState().document!.workflows[0].name).toBe('Renamed');
    const staged = store.getState().document;
    store.getState().setFormError('configuration:pending', 'Apply fields');
    store.getState().undo();
    expect(store.getState().document).toBe(staged);
    store.getState().setFormError('configuration:pending', null);
    const captured = JSON.stringify(store.getState().document);
    store.setState({ mode: 'view' });
    store.getState().command({
      type: 'scenario.configure',
      configuration: { ...configuration(document), environment: { variables: { changed: true } } },
    });
    expect(JSON.stringify(store.getState().document)).toBe(captured);
  });
  it('keeps the current typed document when validation returns normalized data, and rejects stale replies', async () => {
    let finish!: (value: unknown) => void;
    const validate = vi.fn().mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        })
    );
    const store = createScenarioStore({ validate, open: vi.fn(), save: vi.fn() });
    const document = request();
    store.setState({ document, mode: 'edit' });
    const pending = store.getState().validate();
    store.getState().setFormError('configuration:pending', 'Apply fields');
    finish({ valid: true, document: structuredClone(document), diagnostics: [] });
    await pending;
    expect(store.getState().document).toBe(document);
    expect(store.getState().formErrors).toHaveProperty('configuration:pending');
    const stale = store.getState().validate();
    store.getState().command({ type: 'name', name: 'Newer edit' });
    finish({ valid: true, document, diagnostics: [] });
    await stale;
    expect(store.getState().document!.name).toBe('Newer edit');
    expect(store.getState().status).toBe('unchecked');
  });
});
