import { describe, expect, it, vi } from 'vitest';
import { createScenarioStore, exampleDocument } from './store';
import type { ValidationResult } from '../../lib/scenario-types';

const valid = (): ValidationResult => ({ valid: true, document: structuredClone(exampleDocument), diagnostics: [] });
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
function bridge() {
  return {
    validate: vi.fn().mockResolvedValue(valid()),
    open: vi.fn().mockResolvedValue(null),
    save: vi.fn().mockResolvedValue(true),
  };
}

describe('scenario state', () => {
  it('tracks pending validation and clears the loader on completion and failure', async () => {
    const pending = deferred<ValidationResult>();
    const api = bridge();
    api.validate.mockReturnValueOnce(pending.promise);
    const store = createScenarioStore(api);
    const operation = store.getState().validate();
    expect(store.getState().validating).toBe(true);
    pending.resolve(valid());
    await operation;
    expect(store.getState().validating).toBe(false);
    api.validate.mockRejectedValueOnce(new Error('failed'));
    await store.getState().validate();
    expect(store.getState().validating).toBe(false);
  });

  it('clears the validation loader when opening is cancelled and does not enable it for saving', async () => {
    const api = bridge();
    const store = createScenarioStore(api);
    const opening = store.getState().open();
    expect(store.getState().validating).toBe(true);
    await opening;
    expect(store.getState().validating).toBe(false);
    const pending = deferred<boolean>();
    api.save.mockReturnValueOnce(pending.promise);
    const saving = store.getState().save();
    expect(store.getState().busy).toBe(true);
    expect(store.getState().validating).toBe(false);
    pending.resolve(true);
    await saving;
  });
  it('uses validation returned by opening an invalid file without a second validation request', async () => {
    const api = bridge();
    api.open.mockResolvedValue({
      path: '/invalid-json.interl.json',
      format: 'json',
      source: '{broken',
      validation: {
        valid: false,
        document: null,
        diagnostics: [
          {
            category: 'parse',
            code: 'invalidJson',
            message: 'Invalid JSON',
            fieldPath: '$',
            nodeId: null,
            filePath: '/invalid-json.interl.json',
            line: 1,
            column: 2,
          },
        ],
      },
    });
    const store = createScenarioStore(api);
    await store.getState().open();
    expect(store.getState().status).toBe('invalid');
    expect(store.getState().document).toBeNull();
    expect(store.getState().diagnostics[0].code).toBe('invalidJson');
    expect(store.getState().fileRevision).toBe(1);
    expect(api.validate).not.toHaveBeenCalled();
  });
  it('discards a stale validation response after layout changes', async () => {
    const pending = deferred<ValidationResult>();
    const api = bridge();
    api.validate.mockReturnValue(pending.promise);
    const store = createScenarioStore(api);
    const operation = store.getState().validate();
    store.getState().move('health', { x: 999, y: 99 });
    pending.resolve(valid());
    await operation;
    expect(store.getState().document?.layout?.positions.health).toEqual({ x: 999, y: 99 });
    expect(store.getState().busy).toBe(false);
  });
  it('discards stale responses when a newer file is opened and preserves YAML metadata', async () => {
    const pending = deferred<ValidationResult>();
    const api = bridge();
    api.validate.mockReturnValue(pending.promise);
    const next = valid();
    next.document!.name = 'Opened scenario';
    api.open.mockResolvedValue({
      path: '/scenario.interl.yaml',
      format: 'yaml',
      source: 'original YAML',
      validation: next,
    });
    const store = createScenarioStore(api);
    const old = store.getState().validate();
    await store.getState().open();
    pending.resolve(valid());
    await old;
    expect(store.getState().document?.name).toBe('Opened scenario');
    expect(store.getState().format).toBe('yaml');
    expect(store.getState().originalSource).toBe('original YAML');
  });
  it('does not lose the current document when the file dialog is cancelled', async () => {
    const store = createScenarioStore(bridge());
    const document = store.getState().document;
    await store.getState().open();
    expect(store.getState().document).toBe(document);
    expect(store.getState().busy).toBe(false);
  });
  it('reports bridge failures without clearing the scenario', async () => {
    const api = bridge();
    api.validate.mockRejectedValue(new Error('failed'));
    const store = createScenarioStore(api);
    await store.getState().validate();
    expect(store.getState().document).not.toBeNull();
    expect(store.getState().error).toContain('failed');
    expect(store.getState().busy).toBe(false);
  });
  it('exports a JSON copy without changing the original YAML metadata', async () => {
    const api = bridge();
    api.open.mockResolvedValue({
      path: '/scenario.interl.yaml',
      format: 'yaml',
      source: 'original YAML',
      validation: valid(),
    });
    const store = createScenarioStore(api);
    await store.getState().open();
    await store.getState().save();
    expect(api.save).toHaveBeenCalledWith(expect.any(String), 'hello-api');
    expect(store.getState().format).toBe('yaml');
    expect(store.getState().notice).toContain('JSON copy saved');
  });
});
