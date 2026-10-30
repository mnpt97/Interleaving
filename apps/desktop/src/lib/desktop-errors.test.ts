import { describe, expect, it } from 'vitest';
import { errorMessage } from './desktop';
const diagnostic = {
  category: 'validation',
  code: 'missingSecret',
  message: 'A required environment credential is missing. Environment reference: INTERL_ALICE_PASSWORD.',
  fieldPath: '/identities',
  nodeId: null,
  filePath: null,
  line: null,
  column: null,
};
describe('safe desktop diagnostics', () => {
  it('shows structured codes and public environment references for singles and arrays', () => {
    const text =
      'missingSecret: A required environment credential is missing. Environment reference: INTERL_ALICE_PASSWORD.';
    expect(errorMessage(diagnostic)).toBe(text);
    expect(errorMessage([diagnostic, { ...diagnostic, code: 'emptySecret', message: 'Empty credential.' }])).toBe(
      `${text} emptySecret: Empty credential.`
    );
  });
  it('does not expose unstructured errors or serialize arbitrary backend causes', () => {
    for (const value of [
      'private-marker',
      new Error('private-marker'),
      { message: 'private-marker' },
      JSON.stringify(diagnostic),
    ]) {
      expect(errorMessage(value)).toContain('The desktop operation failed.');
      expect(errorMessage(value)).not.toContain('private-marker');
    }
  });
  it('rejects empty, malformed, and oversized diagnostic arrays', () => {
    for (const value of [[], [diagnostic, null], Array.from({ length: 101 }, () => diagnostic)])
      expect(errorMessage(value)).toContain('The desktop operation failed.');
  });
  it('allows an operation-specific safe fallback', () => {
    expect(errorMessage(new Error('private-marker'), 'Run could not start.')).toBe('Run could not start.');
  });
});
