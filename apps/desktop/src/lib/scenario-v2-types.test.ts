import { describe, expect, it } from 'vitest';
import source from '../../../../scenarios/examples/reusable-auth.interl.json?raw';
import { documentSchema } from './scenario-v2-types';
describe('reusable workflow wire shapes', () => {
  it('preserves versioned workflows, lifecycle bindings, ports, calls and scoped IDs', () => {
    const parsed = documentSchema.parse(JSON.parse(source));
    expect(parsed.workflows).toHaveLength(3);
    expect(parsed.groups[0].lifecycle.iteration.inputs.token).toEqual({
      source: 'lifecycle',
      phase: 'initialize',
      name: 'token',
    });
    expect(documentSchema.parse(JSON.parse(JSON.stringify(parsed)))).toEqual(parsed);
    expect(documentSchema.parse(JSON.parse(source))).toEqual(parsed);
    expect(parsed.workflows[1].nodes[2]).toMatchObject({ type: 'call', config: { workflowId: 'inspect-account' } });
  });
  it('rejects unknown executors, inline credentials and malformed call/header shapes', () => {
    for (const mutate of [
      (v: any) => (v.groups[0].executor.kind = 'rampingVus'),
      (v: any) => (v.identities[0].credentials.password = 'private-marker'),
      (v: any) => (v.workflows[1].nodes[2].config.extra = true),
      (v: any) => (v.workflows[1].nodes[1].config.setHeaders.authorization.value = 123),
    ]) {
      const v = JSON.parse(source);
      mutate(v);
      expect(documentSchema.safeParse(v).success).toBe(false);
    }
  });
});

it('rejects v1 and unknown versions without accepting legacy workflow/count fields', () => {
  for (const schemaVersion of [1, 3, '2', null]) {
    expect(documentSchema.safeParse({ ...JSON.parse(source), schemaVersion }).success).toBe(false);
  }
  const v = JSON.parse(source);
  v.workflow = v.workflows[0];
  expect(documentSchema.safeParse(v).success).toBe(false);
  delete v.workflow;
  v.groups[0].count = 1;
  expect(documentSchema.safeParse(v).success).toBe(false);
});

it('preserves typed semantic drafts for Rust diagnostics instead of hiding invalid scalar values', () => {
  const v = JSON.parse(source);
  v.workflows[1].inputs.token.default = null;
  v.groups[0].lifecycle.iteration.inputs.token = { source: 'literal', value: { invalid: 'scalar' } };
  const parsed = documentSchema.parse(v);
  expect(parsed.workflows[1].inputs.token.default).toBeNull();
  expect(parsed.groups[0].lifecycle.iteration.inputs.token).toEqual(v.groups[0].lifecycle.iteration.inputs.token);
});
