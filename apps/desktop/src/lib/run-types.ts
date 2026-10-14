import { z } from "zod";
const statusSchema = z.enum(["passed", "failed", "cancelled", "error"]);
const failureSchema = z.object({ kind: z.string(), message: z.string() });
const bodySchema = z.object({ value: z.unknown(), bytes: z.number(), truncated: z.boolean(), omitted: z.boolean(), reason: z.string().optional() });
const headersSchema = z.record(z.string(), z.string());
const envelope = { runId: z.string(), sequence: z.number().int().positive(), elapsedNs: z.number().nonnegative(), userId: z.string().nullable(), nodeId: z.string().nullable() };
export const eventSchema = z.discriminatedUnion("type", [
  z.object({ ...envelope, type: z.literal("run.started"), payload: z.object({ scenarioId: z.string() }) }),
  z.object({ ...envelope, type: z.literal("user.spawned"), payload: z.object({}) }),
  z.object({ ...envelope, type: z.literal("step.started"), payload: z.object({ kind: z.string() }) }),
  z.object({ ...envelope, type: z.literal("request.sent"), payload: z.object({ method: z.string(), url: z.string(), headers: headersSchema, body: bodySchema }) }),
  z.object({ ...envelope, type: z.literal("response.received"), payload: z.object({ status: z.number(), headers: headersSchema, body: bodySchema }) }),
  z.object({ ...envelope, type: z.literal("step.finished"), payload: z.object({}) }),
  z.object({ ...envelope, type: z.literal("step.failed"), payload: z.object({ error: failureSchema }) }),
  z.object({ ...envelope, type: z.literal("user.finished"), payload: z.object({ status: statusSchema }) }),
  z.object({ ...envelope, type: z.literal("run.cancelled"), payload: z.object({}) }),
  z.object({ ...envelope, type: z.literal("run.finished"), payload: z.object({ status: statusSchema }) }),
]);
export const reportSchema = z.object({ reportVersion: z.literal(1), runId: z.string(), status: statusSchema, snapshot: z.unknown(), summary: z.object({ requests: z.number(), responses: z.number(), httpErrorResponses: z.number(), elapsedNs: z.number() }), events: z.array(eventSchema).max(2048), error: failureSchema.nullable(), eventsTruncated: z.boolean(), deliveryDroppedEvents: z.number() });
export const runMessageSchema = z.discriminatedUnion("kind", [z.object({ kind: z.literal("event"), event: eventSchema }), z.object({ kind: z.literal("finished"), report: reportSchema }), z.object({ kind: z.literal("failed"), message: z.string() })]);
export type RunEvent = z.infer<typeof eventSchema>;
export type RunReport = z.infer<typeof reportSchema>;
export type RunMessage = z.infer<typeof runMessageSchema>;
