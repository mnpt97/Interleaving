import { z } from 'zod';
const statusSchema = z.enum(['passed', 'failed', 'cancelled', 'error']);
const failureSchema = z.object({ kind: z.string(), message: z.string() });
const bodySchema = z.object({
  value: z.unknown(),
  bytes: z.number(),
  truncated: z.boolean(),
  omitted: z.boolean(),
  reason: z.string().optional(),
});
const headersSchema = z.record(z.string(), z.string());
export const emptyEventContext = {
  groupKey: null,
  phase: null,
  iteration: null,
  workflowKey: null,
  invocationId: null,
  parentInvocationId: null,
  callerNodeKey: null,
  nodeKey: null,
  edgeKey: null,
};
export const contextSchema = z
  .object({
    groupKey: z.string().nullable(),
    phase: z.enum(['initialize', 'iteration', 'cleanup']).nullable(),
    iteration: z.number().int().positive().nullable(),
    workflowKey: z.string().nullable(),
    invocationId: z.string().nullable(),
    parentInvocationId: z.string().nullable(),
    callerNodeKey: z.string().nullable(),
    nodeKey: z.string().nullable(),
    edgeKey: z.string().nullable(),
  })
  .strict();
const graphSchema = z
  .object({
    groups: z.array(
      z
        .object({
          key: z.string(),
          id: z.string(),
          inlinePhases: z.array(z.enum(['initialize','iteration','cleanup'])).max(3).optional(),
          initialize: z.string().nullable(),
          iteration: z.string(),
          cleanup: z.string().nullable(),
        })
        .strict()
    ),
    workflows: z.array(
      z
        .object({
          key: z.string(),
          id: z.string(),
          name: z.string(),
          nodes: z.array(
            z
              .object({
                key: z.string(),
                id: z.string(),
                name: z.string().optional(),
                type: z.enum([
                  'start',
                  'http',
                  'extract',
                  'setSession',
                  'authentication',
                  'call',
                  'end',
                  'exclusiveSplit',
                  'exclusiveMerge',
                ]),
                position: z.object({ x: z.number().finite(), y: z.number().finite() }).strict(),
              })
              .strict()
          ),
          edges: z.array(
            z
              .object({
                key: z.string(),
                id: z.string(),
                fromNodeKey: z.string(),
                toNodeKey: z.string(),
                sourceHandle: z.enum(['left', 'right', 'top', 'bottom']).optional(),
                targetHandle: z.enum(['left', 'right', 'top', 'bottom']).optional(),
              })
              .strict()
          ),
        })
        .strict()
    ),
  })
  .strict();
const cleanupSchema = z
  .object({
    status: z.enum(['notConfigured', 'completed', 'error', 'cancelled', 'skipped']),
    error: failureSchema.nullable(),
    reason: z.enum(['safetyLimit', 'ownerClosed', 'receiverClosed']).optional(),
  })
  .strict();
const envelope = {
  runId: z.string(),
  sequence: z.number().int().positive(),
  elapsedNs: z.number().nonnegative(),
  userId: z.string().nullable(),
  nodeId: z.string().nullable(),
  context: contextSchema,
};
export const eventSchema = z.discriminatedUnion('type', [
  z.object({
    ...envelope,
    type: z.literal('lifecycle.started'),
    payload: z.object({ phase: z.enum(['initialize', 'iteration', 'cleanup']) }).strict(),
  }),
  z.object({
    ...envelope,
    type: z.literal('lifecycle.finished'),
    payload: z.object({ phase: z.enum(['initialize', 'iteration', 'cleanup']), status: statusSchema }).strict(),
  }),
  z.object({ ...envelope, type: z.literal('workflow.started'), payload: z.object({}).strict() }),
  z.object({ ...envelope, type: z.literal('workflow.finished'), payload: z.object({ status: statusSchema }).strict() }),
  z.object({ ...envelope, type: z.literal('edge.traversed'), payload: z.object({}).strict() }),
  z.object({
    ...envelope,
    type: z.literal('cleanup.skipped'),
    payload: z.object({ reason: z.enum(['safetyLimit', 'ownerClosed', 'receiverClosed']) }).strict(),
  }),
  z.object({
    ...envelope,
    type: z.literal('route.selected'),
    payload: z.object({ selectedEdgeKey: z.string(), unselectedEdgeKeys: z.array(z.string()) }).strict(),
  }),
  z.object({
    ...envelope,
    type: z.literal('run.started'),
    payload: z.object({ scenarioId: z.string(), graph: graphSchema }),
  }),
  z.object({ ...envelope, type: z.literal('user.spawned'), payload: z.object({}) }),
  z.object({ ...envelope, type: z.literal('step.started'), payload: z.object({ kind: z.string() }) }),
  z.object({
    ...envelope,
    type: z.literal('request.sent'),
    payload: z.object({ method: z.string(), url: z.string(), headers: headersSchema, body: bodySchema }),
  }),
  z.object({
    ...envelope,
    type: z.literal('response.received'),
    payload: z.object({ status: z.number(), headers: headersSchema, body: bodySchema }),
  }),
  z.object({
    ...envelope,
    type: z.literal('session.updated'),
    payload: z.object({ setHeaders: z.array(z.string()), removedHeaders: z.array(z.string()) }).strict(),
  }),
  z.object({ ...envelope, type: z.literal('step.finished'), payload: z.object({}) }),
  z.object({ ...envelope, type: z.literal('step.failed'), payload: z.object({ error: failureSchema }) }),
  z.object({ ...envelope, type: z.literal('user.finished'), payload: z.object({ status: statusSchema }) }),
  z.object({ ...envelope, type: z.literal('run.cancelled'), payload: z.object({}) }),
  z.object({ ...envelope, type: z.literal('run.finished'), payload: z.object({ status: statusSchema }) }),
]);
export const reportSchema = z.object({
  reportVersion: z.literal(3),
  graph: graphSchema,
  cleanup: cleanupSchema,
  runId: z.string(),
  status: statusSchema,
  snapshot: z.unknown(),
  summary: z.object({
    requests: z.number(),
    responses: z.number(),
    httpErrorResponses: z.number(),
    elapsedNs: z.number(),
  }),
  events: z.array(eventSchema).max(2048),
  error: failureSchema.nullable(),
  eventsTruncated: z.boolean(),
  deliveryDroppedEvents: z.number(),
});
export const runMessageSchema = z.discriminatedUnion('kind', [
  z.object({ protocolVersion: z.literal(3), kind: z.literal('event'), event: eventSchema }),
  z.object({ protocolVersion: z.literal(3), kind: z.literal('finished'), report: reportSchema }),
  z.object({ protocolVersion: z.literal(3), kind: z.literal('failed'), message: z.string() }),
]);
export type RunEvent = z.infer<typeof eventSchema>;
export type RunReport = z.infer<typeof reportSchema>;
export type RunMessage = z.infer<typeof runMessageSchema>;
