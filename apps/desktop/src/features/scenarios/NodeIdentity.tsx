import type { WorkflowDefinition } from '../../lib/scenario-types';
import { ConfigurationActions, useConfiguration } from './ConfigurationFields';
import { nodeIdError } from './graph';
import { useScenarioStore } from './store';

export function NodeIdentity({ workflow, nodeId }: { workflow: WorkflowDefinition; nodeId: string }) {
  const key = `node-id:${workflow.id}:${nodeId}`;
  const node = workflow.nodes.find((node) => node.id === nodeId)!;
  const values = useConfiguration(key, { id: nodeId, name: node.name ?? '' });
  const otherPending = useScenarioStore((state) =>
    Object.keys(state.formErrors).some(
      (field) =>
        field !== `configuration:${key}` &&
        (['configuration:', 'fragment:', 'authentication:', 'extract:', 'session:', 'baseUrl:'].some((prefix) =>
          field.startsWith(prefix)
        ) ||
          field === 'position' ||
          (field === `http:${workflow.id}:${nodeId}:path` && node.type === 'http' && Boolean(node.config.path)))
    )
  );
  const error = nodeIdError(workflow, nodeId, values.draft.id);
  return (
    <section className="configuration-editor">
      <label>
        Node ID
        <input
          value={values.draft.id}
          aria-invalid={Boolean(error)}
          onChange={(event) => values.setDraft({ ...values.draft, id: event.target.value })}
        />
      </label>
      <label>
        Node name
        <input
          value={values.draft.name}
          placeholder={nodeId}
          onChange={(event) => values.setDraft({ ...values.draft, name: event.target.value })}
        />
      </label>
      {error && (
        <p className="field-error" role="alert">
          {error}
        </p>
      )}
      <ConfigurationActions
        pending={values.pending}
        disabled={
          Boolean(error) || values.fragments || otherPending || new TextEncoder().encode(values.draft.name).length > 256
        }
        discard={values.discard}
        apply={() => {
          useScenarioStore.getState().command({
            type: 'node.rename',
            workflowId: workflow.id,
            nodeId,
            nextId: values.draft.id,
            name: values.draft.name,
          });
        }}
      />
    </section>
  );
}
