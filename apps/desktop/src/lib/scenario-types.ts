import { z } from 'zod';

// Wire-shape checks only. Scenario semantics and acceptance belong to Rust.
const headers = z.record(z.string(), z.string());
export const secretReferenceSchema = z.object({ source: z.literal('env'), name: z.string() }).strict();
export const templateValueSchema = z.discriminatedUnion('source', [
  z.object({ source: z.literal('literal'), value: z.json() }).strict(),
  z.object({ source: z.literal('identity'), field: z.string() }).strict(),
  z.object({ source: z.literal('extraction'), nodeId: z.string(), name: z.string() }).strict(),
  z
    .object({
      source: z.literal('object'),
      get fields() {
        return z.record(z.string(), templateValueSchema);
      },
    })
    .strict(),
  z
    .object({
      source: z.literal('array'),
      get items() {
        return z.array(templateValueSchema);
      },
    })
    .strict(),
]);
export const sessionHeaderSchema = z.discriminatedUnion('source', [
  z.object({ source: z.literal('literal'), value: z.string(), prefix: z.string().optional() }).strict(),
  z.object({ source: z.literal('identity'), field: z.string(), prefix: z.string().optional() }).strict(),
  z
    .object({ source: z.literal('extraction'), nodeId: z.string(), name: z.string(), prefix: z.string().optional() })
    .strict(),
]);
const body = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('json'), value: z.json() }),
  z.object({ kind: z.literal('text'), value: z.string() }),
  z.object({ kind: z.literal('jsonTemplate'), fields: z.record(z.string(), templateValueSchema) }).strict(),
]);
const httpConfig = z.object({
  method: z.enum(['GET', 'POST', 'PUT', 'PATCH', 'DELETE']),
  path: z.string(),
  headers: headers.default({}),
  timeoutMs: z.number().nullable().optional(),
  body: body.nullable().optional(),
});
export const workflowNodeSchema = z.discriminatedUnion('type', [
  z.object({ id: z.string(), type: z.literal('start') }),
  z.object({ id: z.string(), type: z.literal('http'), config: httpConfig }),
  z.object({ id: z.string(), type: z.literal('end') }),
  z
    .object({
      id: z.string(),
      type: z.literal('extract'),
      config: z.object({ sourceNodeId: z.string(), expression: z.string(), output: z.string() }).strict(),
    })
    .strict(),
  z
    .object({
      id: z.string(),
      type: z.literal('setSession'),
      config: z
        .object({
          setHeaders: z.record(z.string(), sessionHeaderSchema).default({}),
          removeHeaders: z.array(z.string()).default([]),
        })
        .strict(),
    })
    .strict(),
]);
export const documentSchema = z.object({
  schemaVersion: z.literal(1),
  id: z.string(),
  name: z.string(),
  environment: z.object({ variables: z.record(z.string(), z.json()) }),
  clientProfiles: z.array(
    z.object({ id: z.string(), baseUrl: z.string(), timeoutMs: z.number(), headers: headers.default({}) })
  ),
  identities: z.array(z.object({ id: z.string(), credentials: z.record(z.string(), secretReferenceSchema) }).strict()),
  assertions: z.array(z.never()),
  groups: z.array(
    z.object({
      id: z.string(),
      clientProfileId: z.string(),
      count: z.number(),
      identityAssignment: z.object({ mode: z.literal('shared'), identityId: z.string().nullable() }),
    })
  ),
  workflow: z.object({
    nodes: z.array(workflowNodeSchema),
    edges: z.array(z.object({ from: z.string(), to: z.string() })),
  }),
  layout: z
    .object({ positions: z.record(z.string(), z.object({ x: z.number(), y: z.number() })) })
    .nullable()
    .optional(),
});
export const diagnosticSchema = z.object({
  category: z.string(),
  code: z.string(),
  message: z.string(),
  fieldPath: z.string(),
  nodeId: z.string().nullable(),
  filePath: z.string().nullable(),
  line: z.number().nullable(),
  column: z.number().nullable(),
  expressionLine: z.number().nullable().optional(),
  expressionColumn: z.number().nullable().optional(),
});
export const validationSchema = z.object({
  valid: z.boolean(),
  document: documentSchema.nullable(),
  diagnostics: z.array(diagnosticSchema),
});
export const openSchema = z.object({
  path: z.string(),
  format: z.enum(['json', 'yaml']),
  source: z.string(),
  validation: validationSchema,
});
export type ScenarioDocument = z.infer<typeof documentSchema>;
export type WorkflowNode = z.infer<typeof workflowNodeSchema>;
export type Diagnostic = z.infer<typeof diagnosticSchema>;
export type ValidationResult = z.infer<typeof validationSchema>;
export type OpenScenarioResult = z.infer<typeof openSchema>;
export type ScenarioFormat = 'json' | 'yaml';
