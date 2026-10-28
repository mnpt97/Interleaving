import type { WorkflowDefinition, WorkflowNode, ScenarioDocument } from '../../lib/scenario-types';
import {
  ConfigurationActions,
  JsonField,
  NumberField,
  RecordFields,
  SourceEditor,
  TextMap,
  useConfiguration,
  type Template,
} from './ConfigurationFields';
import { useScenarioStore } from './store';

type Http = Extract<WorkflowNode, { type: 'http' }>;
export function HttpConfiguration({
  node,
  workflow,
  document,
  path,
}: {
  node: Http;
  workflow: WorkflowDefinition;
  document: ScenarioDocument;
  path: string;
}) {
  const values = useConfiguration(`http:${workflow.id}:${node.id}`, {
    headers: node.config.headers,
    timeoutMs: node.config.timeoutMs ?? null,
    body: node.config.body ?? null,
  });
  const { draft, setDraft } = values;
  const scope = { document, workflow, nodeId: node.id };
  return (
    <section className="configuration-editor" key={values.epoch}>
      <h3>Request configuration</h3>
      <TextMap
        label="Request headers"
        path={`${path}/headers`}
        value={draft.headers}
        onChange={(headers) => setDraft({ ...draft, headers })}
      />
      <NumberField
        label="Request timeout (ms)"
        value={draft.timeoutMs}
        path={`${path}/timeoutMs`}
        optional
        onChange={(timeoutMs) => setDraft({ ...draft, timeoutMs })}
      />
      <label>
        Body format
        <select
          data-field-path={`${path}/body`}
          value={draft.body?.kind ?? ''}
          onChange={(event) =>
            setDraft({
              ...draft,
              body:
                event.target.value === 'json'
                  ? { kind: 'json', value: {} }
                  : event.target.value === 'text'
                    ? { kind: 'text', value: '' }
                    : event.target.value === 'jsonTemplate'
                      ? { kind: 'jsonTemplate', fields: {} }
                      : null,
            })
          }
        >
          <option value="">No body</option>
          <option value="text">Literal text</option>
          <option value="json">Literal JSON</option>
          <option value="jsonTemplate">JSON template</option>
        </select>
      </label>
      {draft.body?.kind === 'text' && (
        <label>
          Text body
          <textarea
            data-field-path={`${path}/body/value`}
            rows={6}
            value={draft.body.value}
            onChange={(event) => setDraft({ ...draft, body: { kind: 'text', value: event.target.value } })}
          />
        </label>
      )}
      {draft.body?.kind === 'json' && (
        <JsonField
          label="JSON body"
          value={draft.body.value}
          path={`${path}/body/value`}
          onChange={(value) =>
            setDraft({
              ...draft,
              body: { kind: 'json', value: value as Extract<Template, { source: 'literal' }>['value'] },
            })
          }
        />
      )}
      {draft.body?.kind === 'jsonTemplate' && (
        <RecordFields
          label="Template fields"
          value={draft.body.fields}
          path={`${path}/body/fields`}
          create={() => ({ source: 'literal', value: '' }) as Template}
          onChange={(fields) => setDraft({ ...draft, body: { kind: 'jsonTemplate', fields } })}
        >
          {(value, change, field) => (
            <SourceEditor
              value={value}
              onChange={(next) => change(next as Template)}
              path={field}
              scope={scope}
              template
            />
          )}
        </RecordFields>
      )}
      <ConfigurationActions
        pending={values.pending || values.fragments}
        disabled={values.fragments}
        discard={values.discard}
        apply={() =>
          useScenarioStore.getState().command({
            type: 'node.put',
            workflowId: workflow.id,
            node: { ...node, config: { ...node.config, ...draft } },
          })
        }
      />
      <h3>Response outputs</h3>
      <p className="muted">
        Use HTTP response status as a typed source, or add an Extract node to read a response property. Response
        assertions are not available yet.
      </p>
    </section>
  );
}
