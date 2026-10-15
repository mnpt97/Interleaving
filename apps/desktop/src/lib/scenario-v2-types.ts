import { z } from 'zod';
const sourceOptions = [
  z.object({ source: z.literal('literal'), value: z.json() }).strict(),
  z.object({ source: z.literal('environment'), name: z.string() }).strict(),
  z.object({ source: z.literal('identity'), field: z.string() }).strict(),
  z.object({ source: z.literal('input'), name: z.string() }).strict(),
  z.object({ source: z.literal('extraction'), nodeId: z.string(), name: z.string() }).strict(),
  z.object({ source: z.literal('callOutput'), nodeId: z.string(), name: z.string() }).strict(),
  z.object({ source: z.literal('response'), nodeId: z.string(), field: z.literal('status') }).strict(),
] as const;
const source = z.discriminatedUnion('source', [
  ...sourceOptions,
  z.object({ source: z.literal('lifecycle'), phase: z.literal('initialize'), name: z.string() }).strict(),
]);
export const templateValueSchema = z.discriminatedUnion('source', [
  z.object({ source: z.literal('literal'), value: z.json() }).strict(),
  ...sourceOptions.slice(1),
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
const prefix = { prefix: z.string().optional() };
export const sessionHeaderSchema = z.discriminatedUnion('source', [
  z.object({ source: z.literal('literal'), value: z.string(), ...prefix }).strict(),
  z.object({ source: z.literal('environment'), name: z.string(), ...prefix }).strict(),
  z.object({ source: z.literal('identity'), field: z.string(), ...prefix }).strict(),
  z.object({ source: z.literal('input'), name: z.string(), ...prefix }).strict(),
  z.object({ source: z.literal('extraction'), nodeId: z.string(), name: z.string(), ...prefix }).strict(),
  z.object({ source: z.literal('callOutput'), nodeId: z.string(), name: z.string(), ...prefix }).strict(),
]);
const binding = z.object({ workflowId: z.string(), inputs: z.record(z.string(), source).default({}) }).strict();
export const workflowNodeSchema = z.discriminatedUnion('type', [
  z.object({ id: z.string(), type: z.literal('start') }).strict(),
  z.object({ id: z.string(), type: z.literal('end') }).strict(),
  z
    .object({
      id: z.string(),
      type: z.literal('http'),
      config: z
        .object({
          method: z.enum(['GET', 'POST', 'PUT', 'PATCH', 'DELETE']),
          path: z.string(),
          headers: z.record(z.string(), z.string()).default({}),
          timeoutMs: z.number().nullable().optional(),
          body: z
            .discriminatedUnion('kind', [
              z.object({ kind: z.literal('json'), value: z.json() }).strict(),
              z.object({ kind: z.literal('text'), value: z.string() }).strict(),
              z.object({ kind: z.literal('jsonTemplate'), fields: z.record(z.string(), templateValueSchema) }).strict(),
            ])
            .nullable()
            .optional(),
        })
        .strict(),
    })
    .strict(),
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
  z.object({ id: z.string(), type: z.literal('call'), config: binding }).strict(),
  z
    .object({
      id: z.string(),
      type: z.literal('exclusiveSplit'),
      config: z.object({ sourceNodeId: z.string(), mergeNodeId: z.string() }).strict(),
    })
    .strict(),
  z
    .object({
      id: z.string(),
      type: z.literal('exclusiveMerge'),
      config: z.object({ splitNodeId: z.string() }).strict(),
    })
    .strict(),
]);
export const documentSchema = z
  .object({
    schemaVersion: z.literal(2),
    id: z.string(),
    name: z.string(),
    environment: z.object({ variables: z.record(z.string(), z.json()) }).strict(),
    clientProfiles: z.array(
      z
        .object({
          id: z.string(),
          baseUrl: z.string(),
          timeoutMs: z.number(),
          headers: z.record(z.string(), z.string()).default({}),
        })
        .strict()
    ),
    identities: z.array(
      z
        .object({
          id: z.string(),
          credentials: z.record(z.string(), z.object({ source: z.literal('env'), name: z.string() }).strict()),
        })
        .strict()
    ),
    assertions: z.array(z.never()),
    groups: z.array(
      z
        .object({
          id: z.string(),
          clientProfileId: z.string(),
          identityAssignment: z.object({ mode: z.literal('shared'), identityId: z.string().nullable() }).strict(),
          executor: z.object({ kind: z.literal('once') }).strict(),
          lifecycle: z
            .object({
              initialize: binding.nullable().default(null),
              iteration: binding,
              cleanup: binding.nullable().default(null),
              cleanupTimeoutMs: z.number().default(2000),
            })
            .strict(),
        })
        .strict()
    ),
    workflows: z.array(
      z
        .object({
          id: z.string(),
          name: z.string(),
          inputs: z
            .record(
              z.string(),
              z.object({ type: z.enum(['string', 'integer', 'boolean']), default: z.json().optional() }).strict()
            )
            .default({}),
          outputs: z
            .record(z.string(), z.object({ type: z.enum(['string', 'integer', 'boolean']), value: source }).strict())
            .default({}),
          nodes: z.array(workflowNodeSchema),
          edges: z.array(
            z
              .object({
                id: z.string(),
                from: z.string(),
                to: z.string(),
                condition: z.object({ expression: z.string() }).strict().optional(),
                default: z.boolean().optional(),
              })
              .strict()
          ),
          layout: z
            .object({
              positions: z.record(z.string(), z.object({ x: z.number().finite(), y: z.number().finite() }).strict()),
            })
            .strict()
            .nullable()
            .optional(),
        })
        .strict()
    ),
  })
  .strict();
