import type { WorkflowNode } from '../../lib/scenario-types';

export type ExtractionConfig = Extract<WorkflowNode, { type: 'extract' }>['config'];
export type ExtractionOutput = { expression: string; type: 'string' | 'integer' | 'boolean' };

/** Present both supported file shapes as named outputs throughout the editor. */
export function extractionOutputs(config: ExtractionConfig): Record<string, ExtractionOutput> {
  return 'outputs' in config
    ? config.outputs
    : { [config.output]: { expression: config.expression, type: config.type } };
}
