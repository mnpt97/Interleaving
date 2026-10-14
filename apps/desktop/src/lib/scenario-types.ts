import { z } from "zod";

// Wire-shape checks only. Scenario semantics and acceptance belong to Rust.
const headers = z.record(z.string(), z.string());
const body = z.discriminatedUnion("kind", [z.object({ kind: z.literal("json"), value: z.json() }), z.object({ kind: z.literal("text"), value: z.string() })]);
const httpConfig = z.object({ method: z.enum(["GET", "POST", "PUT", "PATCH", "DELETE"]), path: z.string(), headers: headers.default({}), timeoutMs: z.number().nullable().optional(), body: body.nullable().optional() });
export const workflowNodeSchema = z.discriminatedUnion("type", [z.object({ id: z.string(), type: z.literal("start") }), z.object({ id: z.string(), type: z.literal("http"), config: httpConfig }), z.object({ id: z.string(), type: z.literal("end") })]);
export const documentSchema = z.object({
  schemaVersion: z.literal(1), id: z.string(), name: z.string(), environment: z.object({ variables: z.record(z.string(), z.json()) }),
  clientProfiles: z.array(z.object({ id: z.string(), baseUrl: z.string(), timeoutMs: z.number(), headers: headers.default({}) })),
  identities: z.array(z.never()), assertions: z.array(z.never()),
  groups: z.array(z.object({ id: z.string(), clientProfileId: z.string(), count: z.number(), identityAssignment: z.object({ mode: z.literal("shared"), identityId: z.string().nullable() }) })),
  workflow: z.object({ nodes: z.array(workflowNodeSchema), edges: z.array(z.object({ from: z.string(), to: z.string() })) }),
  layout: z.object({ positions: z.record(z.string(), z.object({ x: z.number(), y: z.number() })) }).nullable().optional(),
});
export const diagnosticSchema = z.object({ category: z.string(), code: z.string(), message: z.string(), fieldPath: z.string(), nodeId: z.string().nullable(), filePath: z.string().nullable(), line: z.number().nullable(), column: z.number().nullable() });
export const validationSchema = z.object({ valid: z.boolean(), document: documentSchema.nullable(), diagnostics: z.array(diagnosticSchema) });
export const openSchema = z.object({ path: z.string(), format: z.enum(["json", "yaml"]), source: z.string(), validation: validationSchema });
export type ScenarioDocument = z.infer<typeof documentSchema>;
export type WorkflowNode = z.infer<typeof workflowNodeSchema>;
export type Diagnostic = z.infer<typeof diagnosticSchema>;
export type ValidationResult = z.infer<typeof validationSchema>;
export type OpenScenarioResult = z.infer<typeof openSchema>;
export type ScenarioFormat = "json" | "yaml";
