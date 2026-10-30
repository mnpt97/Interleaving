import { afterEach, describe, expect, it, vi } from 'vitest';
import jsonSource from '../../../../../scenarios/examples/auth-login.interl.json?raw';
import yamlSource from '../../../../../scenarios/examples/auth-login.interl.yaml?raw';
import { documentSchema } from '../../lib/scenario-types';
import { openScenarioFile, validateSource } from '../../lib/desktop';
import { createScenarioStore } from './store';
import { moveNode, toFlowEdges, toFlowNodes } from './adapters';
import { nodePresentation } from './node-presentation';
import { createRunStore } from '../runs/store';

const bridge = vi.hoisted(() => ({ read: vi.fn(), open: vi.fn(), validate: vi.fn() }));
vi.mock('../../lib/desktop', async (original) => ({
  ...(await original<typeof import('../../lib/desktop')>()),
  openScenarioFile: bridge.open,
  validateSource: bridge.validate,
  readScenarioFile: bridge.read,
}));
const document = documentSchema.parse(JSON.parse(jsonSource));
afterEach(() => vi.clearAllMocks());

describe('auth loading through the desktop wire boundary', () => {
  it.each([
    ['json', jsonSource],
    ['yaml', yamlSource],
  ] as const)(
    'replaces the workflow from a backend-validated %s file and saves all references',
    async (format, source) => {
      // YAML parsing remains in the backend; the bridge receives its canonical typed document.
      const path = `/auth-login.interl.${format}`;
      bridge.open.mockResolvedValue({
        path,
        format,
        source,
        validation: { valid: true, document, diagnostics: [] },
      });
      const save = vi.fn().mockResolvedValue(true);
      const store = createScenarioStore({ open: openScenarioFile, validate: validateSource, save });
      await store.getState().open();
      expect(store.getState()).toMatchObject({
        status: 'valid',
        format,
        originalSource: source,
        selectedId: 'start',
        fileRevision: 1,
      });
      expect(store.getState().document).toEqual(document);
      expect(toFlowNodes(store.getState().document!, null).map((node) => node.data.node.type)).toEqual([
        'start',
        'http',
        'extract',
        'setSession',
        'http',
        'end',
      ]);
      const workflow = store.getState().document!.workflows[0];
      await store.getState().setMode('edit');
      store.getState().move('extract-token', { x: 900, y: 300 });
      expect(store.getState().document!.workflows[0].nodes).toBe(workflow.nodes);
      await store.getState().save();
      expect(JSON.parse(save.mock.calls[0][0])).toEqual(store.getState().document);
      expect(store.getState().format).toBe(format);
      expect(store.getState().originalSource).toBe(source);
      expect(bridge.open).toHaveBeenCalledTimes(1);
    }
  );
  it('keeps auth fields and extraction references outside React Flow layout data', () => {
    const moved = moveNode(document, 'apply-token', { x: 999, y: 44 });
    expect(moved.identities).toBe(document.identities);
    expect(moved.workflows[0].nodes).toBe(document.workflows[0].nodes);
    expect(toFlowEdges(moved)).toEqual(toFlowEdges(document));
    expect(nodePresentation.extract.label).toBe('Extract');
    expect(nodePresentation.setSession.label).toBe('Set session');
  });
  it('captures auth references at start without letting later file or layout edits mutate active input', async () => {
    const start = vi.fn().mockResolvedValue({ runId: 'auth-run' });
    const run = createRunStore({ start, cancel: vi.fn(), save: vi.fn() });
    const editable = structuredClone(document);
    await run.getState().start(editable);
    editable.identities[0].credentials.password.name = 'CHANGED_REFERENCE';
    editable.workflows[0].nodes.splice(1, 1);
    expect(JSON.parse(start.mock.calls[0][0])).toEqual(document);
    expect(run.getState().scenarioId).toBe('auth-login');
    expect(run.getState().status).toBe('running');
  });
});
