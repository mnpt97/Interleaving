import type { ScenarioDocument, WorkflowDefinition, WorkflowNode } from '../../lib/scenario-types';
import { outputConsumers } from './configuration';
import { canCall } from './graph';
import { useScenarioStore } from './store';
import {
  ConfigurationActions,
  JsonField,
  RecordFields,
  SourceEditor,
  useConfiguration,
  type Scalar,
  type Scope,
} from './ConfigurationFields';

export function MappingFields({
  value,
  onChange,
  path,
  scope,
  ports,
}: {
  value: Record<string, Scalar>;
  onChange(value: Record<string, Scalar>): void;
  path: string;
  scope: Scope;
  ports: WorkflowDefinition['inputs'];
}) {
  return (
    <>
      <p className="muted">
        {Object.entries(ports)
          .map(
            ([name, port]) =>
              `${name}: ${port.type}${port.default === undefined ? ' (required)' : ' (default available)'}`
          )
          .join(' · ') || 'This workflow has no input ports.'}
      </p>
      <RecordFields
        label="Input mappings"
        nameOptions={Object.keys(ports)}
        value={value}
        onChange={onChange}
        path={path}
        create={() => ({ source: 'literal', value: '' }) as Scalar}
      >
        {(source, change, field) => (
          <SourceEditor value={source} onChange={(next) => change(next as Scalar)} path={field} scope={scope} />
        )}
      </RecordFields>
    </>
  );
}

export function WorkflowConfiguration({
  workflow,
  document,
  path,
}: {
  workflow: WorkflowDefinition;
  document: ScenarioDocument;
  path: string;
}) {
  const values = useConfiguration(`workflow:${workflow.id}`, {
    name: workflow.name,
    inputs: workflow.inputs,
    outputs: workflow.outputs,
  });
  const { draft, setDraft } = values;
  return (
    <section className="configuration-editor" key={values.epoch}>
      <h3>Workflow definition</h3>
      <p className="muted">Workflow ID: {workflow.id}</p>
      <label>
        Workflow name
        <input
          data-field-path={`${path}/name`}
          value={draft.name}
          onChange={(event) => setDraft({ ...draft, name: event.target.value })}
        />
      </label>
      <RecordFields
        label="Input ports"
        value={draft.inputs}
        path={`${path}/inputs`}
        create={() => ({ type: 'string' as const })}
        onChange={(inputs) => setDraft({ ...draft, inputs })}
      >
        {(port, change, field) => (
          <>
            <label>
              Input type
              <select
                value={port.type}
                onChange={(event) => change({ ...port, type: event.target.value as typeof port.type })}
              >
                {['string', 'integer', 'boolean'].map((type) => (
                  <option key={type}>{type}</option>
                ))}
              </select>
            </label>
            <label>
              <input
                type="checkbox"
                checked={port.default !== undefined}
                onChange={(event) => {
                  const next = { ...port };
                  if (event.target.checked)
                    next.default = port.type === 'boolean' ? false : port.type === 'integer' ? 0 : '';
                  else delete next.default;
                  change(next);
                }}
              />
              Has default
            </label>
            {port.default !== undefined && (
              <JsonField
                label="Default value"
                path={`${field}/default`}
                value={port.default}
                scalar
                onChange={(value) => change({ ...port, default: value as typeof port.default })}
              />
            )}
          </>
        )}
      </RecordFields>
      <RecordFields
        label="Output ports"
        value={draft.outputs}
        path={`${path}/outputs`}
        create={() => ({ type: 'string' as const, value: { source: 'literal' as const, value: '' } })}
        onChange={(outputs) => setDraft({ ...draft, outputs })}
      >
        {(port, change, field) => (
          <>
            <label>
              Output type
              <select
                value={port.type}
                onChange={(event) => change({ ...port, type: event.target.value as typeof port.type })}
              >
                {['string', 'integer', 'boolean'].map((type) => (
                  <option key={type}>{type}</option>
                ))}
              </select>
            </label>
            <p className="muted">
              Consumers:{' '}
              {outputConsumers(
                document,
                workflow.id,
                field.split('/').pop()!.replace(/~1/g, '/').replace(/~0/g, '~')
              ).join(', ') || 'None yet'}
            </p>
            <SourceEditor
              value={port.value}
              onChange={(value) => change({ ...port, value: value as Scalar })}
              path={`${field}/value`}
              scope={{ document, workflow: { ...workflow, inputs: draft.inputs } }}
            />
          </>
        )}
      </RecordFields>
      <ConfigurationActions
        pending={values.pending || values.fragments}
        disabled={values.fragments || !draft.name.trim()}
        discard={values.discard}
        apply={() =>
          useScenarioStore
            .getState()
            .command({ type: 'workflow.configure', workflowId: workflow.id, configuration: draft })
        }
      />
    </section>
  );
}

export function CallConfiguration({
  node,
  workflow,
  document,
  path,
}: {
  node: Extract<WorkflowNode, { type: 'call' }>;
  workflow: WorkflowDefinition;
  document: ScenarioDocument;
  path: string;
}) {
  const values = useConfiguration(`call:${workflow.id}:${node.id}`, node.config);
  const { draft, setDraft } = values;
  const callee = document.workflows.find((workflow) => workflow.id === draft.workflowId);
  return (
    <section className="configuration-editor" key={values.epoch}>
      <h3>Call configuration</h3>
      <label>
        Called workflow
        <select
          data-field-path={`${path}/workflowId`}
          value={draft.workflowId}
          onChange={(event) => setDraft({ ...draft, workflowId: event.target.value })}
        >
          {!callee && <option value={draft.workflowId}>Missing: {draft.workflowId}</option>}
          {document.workflows
            .filter((candidate) => candidate.id === draft.workflowId || canCall(document, workflow.id, candidate.id))
            .map((candidate) => (
              <option key={candidate.id} value={candidate.id}>
                {candidate.name}
              </option>
            ))}
        </select>
      </label>
      <MappingFields
        value={draft.inputs}
        onChange={(inputs) => setDraft({ ...draft, inputs })}
        path={`${path}/inputs`}
        ports={callee?.inputs ?? {}}
        scope={{ document, workflow, nodeId: node.id }}
      />
      <h3>Published outputs</h3>
      <p className="muted">
        {Object.entries(callee?.outputs ?? {})
          .map(([name, port]) => `${name}: ${port.type}`)
          .join(' · ') || 'No declared outputs.'}{' '}
      </p>
      <ConfigurationActions
        pending={values.pending || values.fragments}
        disabled={values.fragments}
        discard={values.discard}
        apply={() =>
          useScenarioStore
            .getState()
            .command({ type: 'node.put', workflowId: workflow.id, node: { ...node, config: draft } })
        }
      />
    </section>
  );
}
