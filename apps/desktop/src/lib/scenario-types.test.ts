import { describe, expect, it } from 'vitest';
import authSource from '../../../../scenarios/examples/auth-login.interl.json?raw';
import { diagnosticSchema, documentSchema, sessionHeaderSchema, templateValueSchema } from './scenario-types';

const authDocument = documentSchema.parse(JSON.parse(authSource));
describe('M3 scenario wire contract', () => {
  it('retains every authentication field through a JSON round trip', () => {
    const parsed = documentSchema.parse(JSON.parse(JSON.stringify(authDocument)));
    expect(parsed).toEqual(authDocument);
    expect(parsed.identities[0].credentials.password).toEqual({ source: 'env', name: 'INTERL_ALICE_PASSWORD' });
    expect(parsed.workflow.nodes.map((node) => node.type)).toEqual([
      'start',
      'http',
      'extract',
      'setSession',
      'http',
      'end',
    ]);
    expect(parsed.workflow.nodes[3]).toMatchObject({
      config: {
        setHeaders: {
          authorization: { source: 'extraction', nodeId: 'extract-token', name: 'token', prefix: 'Bearer ' },
        },
      },
    });
  });
  it('accepts nested explicit templates without interpreting literal objects or strings', () => {
    const literal = { source: 'literal', value: { source: 'identity', field: '${password}' } };
    const template = {
      source: 'object',
      fields: {
        nested: {
          source: 'array',
          items: [
            literal,
            { source: 'identity', field: 'password' },
            { source: 'extraction', nodeId: 'extract-token', name: 'token' },
          ],
        },
      },
    };
    expect(templateValueSchema.parse(template)).toEqual(template);
    expect(sessionHeaderSchema.parse({ source: 'identity', field: 'token' })).toEqual({
      source: 'identity',
      field: 'token',
    });
  });
  it('rejects inline credentials, unknown providers and malformed auth variants', () => {
    const inline = structuredClone(authDocument) as unknown as { identities: unknown[] };
    inline.identities = [{ id: 'alice', credentials: { password: 'plaintext' } }];
    expect(documentSchema.safeParse(inline).success).toBe(false);
    inline.identities = [{ id: 'alice', credentials: { password: { source: 'keychain', name: 'password' } } }];
    expect(documentSchema.safeParse(inline).success).toBe(false);
    expect(templateValueSchema.safeParse({ source: 'extraction', name: 'token' }).success).toBe(false);
    expect(sessionHeaderSchema.safeParse({ source: 'literal', value: { nested: true } }).success).toBe(false);
    expect(sessionHeaderSchema.safeParse({ source: 'identity', field: 'token', value: 'hidden-extra' }).success).toBe(
      false
    );
  });
  it('preserves separate expression and document locations', () => {
    expect(
      diagnosticSchema.parse({
        category: 'validation',
        code: 'invalidExpression',
        message: 'Unsupported expression',
        fieldPath: '/workflow/nodes/2/config/expression',
        nodeId: 'extract-token',
        filePath: null,
        line: 12,
        column: 3,
        expressionLine: 1,
        expressionColumn: 14,
      })
    ).toMatchObject({ line: 12, expressionLine: 1, expressionColumn: 14 });
  });
});
