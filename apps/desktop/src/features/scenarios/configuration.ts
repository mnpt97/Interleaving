import type { ScenarioDocument, WorkflowDefinition } from '../../lib/scenario-types';
import { extractionOutputs, type ExtractionConfig } from './extraction';
export type ScenarioConfiguration = Pick<ScenarioDocument, 'environment' | 'clientProfiles' | 'identities' | 'groups'>;
export type WorkflowConfiguration = Pick<WorkflowDefinition, 'name' | 'inputs' | 'outputs'>;

/** Traverse executable descriptors, never literal JSON that happens to resemble one. */
function sources(value: unknown, visit: (value: Record<string, unknown>) => void) {
  if (!value || typeof value !== 'object') return;
  const record = value as Record<string, unknown>;
  if (record.source === 'literal' || record.kind === 'json' || record.kind === 'text') return;
  if (typeof record.source === 'string') visit(record);
  Object.entries(record).forEach(([key, item]) => {
    const portDefault = key === 'default' && ['string', 'integer', 'boolean'].includes(String(record.type));
    if (!portDefault) sources(item, visit);
  });
}

/** Keep named extraction consumers valid when outputs are removed or change type. */
export function checkExtractionOutputs(
  workflow: WorkflowDefinition,
  nodeId: string,
  before: ExtractionConfig,
  after: ExtractionConfig
) {
  const previous = extractionOutputs(before);
  const next = extractionOutputs(after);
  sources(workflow, (source) => {
    if (source.source !== 'extraction' || source.nodeId !== nodeId || typeof source.name !== 'string') return;
    const name = source.name;
    if (previous[name] && (!next[name] || next[name].type !== previous[name].type)) {
      throw new Error(
        `Output ${name} is referenced. Remove its mappings before renaming, removing, or changing its type.`
      );
    }
  });
}
function removed(before: object, after: object) {
  const names = Object.keys(after);
  return Object.keys(before).filter((key) => !names.includes(key));
}

export function configureScenario(document: ScenarioDocument, configuration: ScenarioConfiguration) {
  const next = { ...document, ...configuration };
  const missingVariables = removed(document.environment.variables, next.environment.variables);
  const assigned = next.groups[0]?.identityAssignment.identityId;
  const oldIdentity = document.identities.find((identity) => identity.id === assigned);
  const newIdentity = next.identities.find((identity) => identity.id === assigned);
  const previouslyAssigned = document.identities.find(
    (identity) => identity.id === document.groups[0]?.identityAssignment.identityId
  );
  const missingFields = oldIdentity && newIdentity ? removed(oldIdentity.credentials, newIdentity.credentials) : [];
  sources({ workflows: next.workflows, groups: next.groups }, (source) => {
    if (source.source === 'environment' && missingVariables.includes(String(source.name)))
      throw new Error(
        `Scenario variable ${source.name} is referenced. Update its consumers before removing or renaming it.`
      );
    if (
      source.source === 'identity' &&
      (missingFields.includes(String(source.field)) ||
        (previouslyAssigned?.credentials[String(source.field)] && !newIdentity?.credentials[String(source.field)]))
    )
      throw new Error(`Identity field ${source.field} is referenced. Update its consumers first.`);
  });
  for (const workflow of next.workflows)
    for (const node of workflow.nodes) {
      if (node.type === 'http' || node.type === 'authentication')
        for (const name of missingVariables)
          if (node.config.path.includes('${' + name + '}'))
            throw new Error(`Scenario variable ${name} is used in a request path.`);
    }
  for (const group of next.groups) {
    if (
      document.clientProfiles.some((profile) => profile.id === group.clientProfileId) &&
      !next.clientProfiles.some((profile) => profile.id === group.clientProfileId)
    )
      throw new Error('A workload still uses the removed client profile. Select another profile first.');
    if (
      group.identityAssignment.identityId &&
      document.identities.some((identity) => identity.id === group.identityAssignment.identityId) &&
      !next.identities.some((identity) => identity.id === group.identityAssignment.identityId)
    )
      throw new Error('A workload still uses the removed identity. Clear or change the assignment first.');
  }
  return next;
}

export function configureWorkflow(
  document: ScenarioDocument,
  workflowId: string,
  configuration: WorkflowConfiguration
) {
  const workflow = document.workflows.find((workflow) => workflow.id === workflowId);
  if (!workflow) throw new Error('Workflow is unavailable.');
  const missingInputs = removed(workflow.inputs, configuration.inputs);
  const missingOutputs = removed(workflow.outputs, configuration.outputs);
  const nextWorkflow = { ...workflow, ...configuration };
  sources(nextWorkflow, (source) => {
    if (source.source === 'input' && missingInputs.includes(String(source.name)))
      throw new Error(`Input ${source.name} is referenced. Update its consumers before removing or renaming it.`);
  });
  for (const parent of document.workflows)
    for (const node of parent.nodes) {
      if (node.type !== 'call' || node.config.workflowId !== workflowId) continue;
      if (missingInputs.some((name) => name in node.config.inputs))
        throw new Error(`Call ${node.id} still maps a removed input. Update the call mapping first.`);
      sources(parent, (source) => {
        if (source.source === 'callOutput' && source.nodeId === node.id && missingOutputs.includes(String(source.name)))
          throw new Error(`Output ${source.name} is consumed by a call mapping. Update its consumers first.`);
      });
    }
  for (const group of document.groups) {
    for (const binding of [group.lifecycle.initialize, group.lifecycle.iteration, group.lifecycle.cleanup])
      if (binding?.workflowId === workflowId && missingInputs.some((name) => name in binding.inputs))
        throw new Error('A lifecycle binding still maps a removed input. Update the binding first.');
    if (group.lifecycle.initialize?.workflowId === workflowId)
      sources(group, (source) => {
        if (source.source === 'lifecycle' && missingOutputs.includes(String(source.name)))
          throw new Error('An iteration binding still consumes this initialize output.');
      });
  }
  return {
    ...document,
    workflows: document.workflows.map((value) => (value.id === workflowId ? nextWorkflow : value)),
  };
}

export function outputConsumers(document: ScenarioDocument, workflowId: string, output: string): string[] {
  const consumers = new Set<string>();
  for (const workflow of document.workflows)
    for (const call of workflow.nodes) {
      if (call.type !== 'call' || call.config.workflowId !== workflowId) continue;
      for (const node of workflow.nodes)
        sources(node, (source) => {
          if (source.source === 'callOutput' && source.nodeId === call.id && source.name === output)
            consumers.add(`${workflow.name} / ${node.id}`);
        });
      sources(workflow.outputs, (source) => {
        if (source.source === 'callOutput' && source.nodeId === call.id && source.name === output)
          consumers.add(`${workflow.name} outputs`);
      });
    }
  for (const group of document.groups)
    if (group.lifecycle.initialize?.workflowId === workflowId)
      sources(group.lifecycle.iteration, (source) => {
        if (source.source === 'lifecycle' && source.name === output) consumers.add(`${group.id} iteration`);
      });
  return [...consumers];
}
