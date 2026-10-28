import { z } from 'zod';
import contract from '../../../../schemas/scenario.schema.json';
import type { ScenarioFile } from './generated/scenario-file';

/** Generated Rust file contract; the schemas below only normalize editor defaults. */
export const scenarioFileSchema = z.fromJSONSchema(
  contract as Parameters<typeof z.fromJSONSchema>[0]
) as z.ZodType<ScenarioFile>;
export type { ScenarioFile } from './generated/scenario-file';
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
const authenticationFields = {
  baseUrl: z
    .string()
    .nullish()
    .transform((value) => value ?? undefined)
    .optional(),
  path: z.string(),
  method: z.enum(['GET', 'POST', 'PUT', 'PATCH', 'DELETE']),
  username: z.object({ source: z.literal('env'), name: z.string() }).strict(),
  password: z.object({ source: z.literal('env'), name: z.string() }).strict(),
  usernameKey: z.string(),
  passwordKey: z.string(),
};
export const workflowNodeSchema = z.discriminatedUnion('type', [
  z
    .object({
      id: z.string(),
      name: z
        .string()
        .nullish()
        .transform((value) => value ?? undefined)
        .optional(),
      type: z.literal('start'),
    })
    .strict(),
  z
    .object({
      id: z.string(),
      name: z
        .string()
        .nullish()
        .transform((value) => value ?? undefined)
        .optional(),
      type: z.literal('end'),
    })
    .strict(),
  z
    .object({
      id: z.string(),
      name: z
        .string()
        .nullish()
        .transform((value) => value ?? undefined)
        .optional(),
      type: z.literal('http'),
      config: z
        .object({
          method: z.enum(['GET', 'POST', 'PUT', 'PATCH', 'DELETE']),
          path: z.string(),
          baseUrl: z
            .string()
            .nullish()
            .transform((value) => value ?? undefined)
            .optional(),
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
      name: z
        .string()
        .nullish()
        .transform((value) => value ?? undefined)
        .optional(),
      type: z.literal('extract'),
      config: z.union([
        z
          .object({
            sourceNodeId: z.string(),
            outputs: z.record(
              z.string(),
              z
                .object({ expression: z.string(), type: z.enum(['string', 'integer', 'boolean']).default('string') })
                .strict()
            ),
          })
          .strict(),
        z
          .object({
            sourceNodeId: z.string(),
            expression: z.string(),
            output: z.string(),
            type: z.enum(['string', 'integer', 'boolean']).default('string'),
          })
          .strict(),
      ]),
    })
    .strict(),
  z
    .object({
      id: z.string(),
      name: z
        .string()
        .nullish()
        .transform((value) => value ?? undefined)
        .optional(),
      type: z.literal('setSession'),
      config: z
        .object({
          setHeaders: z.record(z.string(), sessionHeaderSchema).default({}),
          removeHeaders: z.array(z.string()).default([]),
        })
        .strict(),
    })
    .strict(),
  z
    .object({
      id: z.string(),
      name: z
        .string()
        .nullish()
        .transform((value) => value ?? undefined)
        .optional(),
      type: z.literal('authentication'),
      config: z.discriminatedUnion('mode', [
        z
          .object({
            ...authenticationFields,
            mode: z.literal('bearer'),
            tokenExpression: z.string(),
            headerName: z.string().default('Authorization'),
            headerPrefix: z.string().default('Bearer '),
          })
          .strict(),
        z.object({ ...authenticationFields, mode: z.literal('cookie') }).strict(),
      ]),
    })
    .strict(),
  z
    .object({
      id: z.string(),
      name: z
        .string()
        .nullish()
        .transform((value) => value ?? undefined)
        .optional(),
      type: z.literal('call'),
      config: binding,
    })
    .strict(),
  z
    .object({
      id: z.string(),
      name: z
        .string()
        .nullish()
        .transform((value) => value ?? undefined)
        .optional(),
      type: z.literal('exclusiveSplit'),
      config: z.object({ sourceNodeId: z.string(), inputs: z.record(z.string(), source).default({}) }).strict(),
    })
    .strict(),
  z
    .object({
      id: z.string(),
      name: z
        .string()
        .nullish()
        .transform((value) => value ?? undefined)
        .optional(),
      type: z.literal('exclusiveMerge'),
      config: z.object({}).strict().default({}),
    })
    .strict(),
]);
const editorDocumentSchema = z
  .object({
    schemaVersion: z.literal(2),
    id: z.string(),
    name: z.string(),
    environment: z
      .object({ variables: z.record(z.string(), z.json()).default({}) })
      .strict()
      .default({ variables: {} }),
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
    identities: z
      .array(
        z
          .object({
            id: z.string(),
            credentials: z.record(z.string(), z.object({ source: z.literal('env'), name: z.string() }).strict()),
          })
          .strict()
      )
      .default([]),
    assertions: z.array(z.never()).default([]),
    groups: z.array(
      z
        .object({
          id: z.string(),
          clientProfileId: z.string(),
          identityAssignment: z
            .object({ mode: z.literal('shared'), identityId: z.string().nullable().default(null) })
            .strict()
            .default({ mode: 'shared', identityId: null }),
          executor: z
            .object({ kind: z.literal('once') })
            .strict()
            .default({ kind: 'once' }),
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
              connections: z
                .record(
                  z.string(),
                  z
                    .object({
                      source: z.enum(['left', 'right', 'top', 'bottom']),
                      target: z.enum(['left', 'right', 'top', 'bottom']),
                    })
                    .strict()
                )
                .optional(),
              positions: z
                .record(z.string(), z.object({ x: z.number().finite(), y: z.number().finite() }).strict())
                .default({}),
            })
            .strict()
            .nullable()
            .optional(),
        })
        .strict()
    ),
  })
  .strict();

export const documentSchema = scenarioFileSchema
  .transform((value) => value as z.input<typeof editorDocumentSchema>)
  .pipe(editorDocumentSchema);
