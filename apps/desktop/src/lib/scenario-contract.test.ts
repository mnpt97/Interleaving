import { expect, it } from 'vitest';
import { documentSchema, scenarioFileSchema } from './scenario-document';
import { executeCommand, newDraft } from '../features/scenarios/draft';

it('accepts all repository JSON examples through the schema-generated structural contract and editor adapter', () => {
  const files = import.meta.glob('../../../../scenarios/examples/*.json', {
    eager: true,
    query: '?raw',
    import: 'default',
  });
  for (const [path, source] of Object.entries(files)) {
    if (path.endsWith('/invalid-json.interl.json')) {
      expect(() => JSON.parse(source as string)).toThrow();
      continue;
    }
    const input = JSON.parse(source as string);
    expect(scenarioFileSchema.safeParse(input).success, path).toBe(true);
    expect(documentSchema.safeParse(input).success, path).toBe(true);
  }
});
it('normalizes omitted default fields for the editor', () => {
  const draft = newDraft();
  const { environment: _environment, identities: _identities, assertions: _assertions, ...input } = draft;
  const { executor: _executor, identityAssignment: _identity, ...group } = input.groups[0];
  const normalized = documentSchema.parse({ ...input, groups: [group] });
  expect(normalized.environment).toEqual({ variables: {} });
  expect(normalized.identities).toEqual([]);
  expect(normalized.groups[0].executor).toEqual({ kind: 'once' });
  expect(normalized.groups[0].identityAssignment).toEqual({ mode: 'shared', identityId: null });
});
it('accepts cookie authentication without bearer-only fields and rejects those irrelevant fields', () => {
  const draft = executeCommand(newDraft(), { type: 'node.add', workflowId: 'workflow-1', kind: 'authentication' });
  const node = draft.workflows[0].nodes.find((node) => node.type === 'authentication')!;
  if (node.type !== 'authentication') throw new Error('fixture');
  const {
    tokenExpression: _token,
    headerName: _header,
    headerPrefix: _prefix,
    ...config
  } = node.config as Extract<typeof node.config, { mode: 'bearer' }>;
  const cookie = { ...node, config: { ...config, mode: 'cookie' } };
  const document = { ...draft, workflows: [{ ...draft.workflows[0], nodes: [cookie] }] };
  expect(documentSchema.safeParse(document).success).toBe(true);
  cookie.config = { ...cookie.config, tokenExpression: 'response.body.token' } as typeof cookie.config;
  expect(scenarioFileSchema.safeParse(document).success).toBe(false);
});
it('defaults bearer header settings while keeping the token expression explicit', () => {
  const draft = executeCommand(newDraft(), { type: 'node.add', workflowId: 'workflow-1', kind: 'authentication' });
  const input = JSON.parse(JSON.stringify(draft));
  const node = input.workflows[0].nodes.find((node: { type: string }) => node.type === 'authentication');
  delete node.config.headerName;
  delete node.config.headerPrefix;
  delete node.name;
  const parsed = documentSchema.parse(input).workflows[0].nodes.find((node) => node.type === 'authentication');
  expect(parsed).toMatchObject({ config: { mode: 'bearer', headerName: 'Authorization', headerPrefix: 'Bearer ' } });
  delete node.config.tokenExpression;
  expect(scenarioFileSchema.safeParse(input).success).toBe(false);
});
