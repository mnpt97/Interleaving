import { z } from 'zod';
import { documentSchema, workflowNodeSchema } from './scenario-v2-types';
export { documentSchema, workflowNodeSchema, templateValueSchema, sessionHeaderSchema } from './scenario-v2-types';
// Wire shapes only. Rust owns semantic validation.
export const diagnosticSchema = z.object({
  category: z.string(),
  code: z.string(),
  message: z.string(),
  fieldPath: z.string(),
  nodeId: z.string().nullable(),
  filePath: z.string().nullable(),
  line: z.number().nullable(),
  column: z.number().nullable(),
  workflowId: z.string().nullable().optional(),
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
export type WorkflowDefinition = ScenarioDocument['workflows'][number];
export type WorkflowNode = z.infer<typeof workflowNodeSchema>;
export type Diagnostic = z.infer<typeof diagnosticSchema>;
export type ValidationResult = z.infer<typeof validationSchema>;
export type OpenScenarioResult = z.infer<typeof openSchema>;
export type ScenarioFormat = 'json' | 'yaml';

// Until workflow selection arrives in Phase 2, display the iteration definition.
export function iterationWorkflow(document: ScenarioDocument): WorkflowDefinition | undefined {
  const id = document.groups[0]?.lifecycle.iteration.workflowId;
  return document.workflows.find((workflow) => workflow.id === id) ?? document.workflows[0];
}
