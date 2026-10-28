import type { WorkflowDefinition, WorkflowNode } from '../../lib/scenario-types';
import { Button } from '../../components/ui/button';
import {
  ConfigurationActions,
  earlierNodes,
  useConfiguration,
  RecordFields,
  SourceEditor,
  type Scalar,
} from './ConfigurationFields';
import { useScenarioStore } from './store';

export function GatewayConfiguration({
  node,
  workflow,
  editing,
  path,
}: {
  node: Extract<WorkflowNode, { type: 'exclusiveSplit' | 'exclusiveMerge' }>;
  workflow: WorkflowDefinition;
  editing: boolean;
  path: string;
}) {
  const split = node.type === 'exclusiveSplit';
  const values = useConfiguration(`gateway:${workflow.id}:${node.id}`, {
    sourceNodeId: split ? node.config.sourceNodeId : '',
    inputs: split ? node.config.inputs : {},
    routes: workflow.edges.filter((edge) => edge.from === node.id),
  });
  const command = useScenarioStore((state) => state.command);
  const document = useScenarioStore((state) => state.document);
  const sources = document
    ? earlierNodes({ document, workflow, nodeId: node.id }).filter((candidate) => candidate.type === 'http')
    : [];
  const { draft, setDraft } = values;
  if (!editing)
    return (
      <section className="configuration-editor">
        <h3>{split ? 'Exclusive routes' : 'Exclusive merge'}</h3>
        {split && <p>Response source: {draft.sourceNodeId}</p>}
        {split && (
          <ul>
            {draft.routes.map((edge) => (
              <li key={edge.id}>
                {edge.to}: {edge.default ? 'Default' : edge.condition?.expression}
              </li>
            ))}
          </ul>
        )}
      </section>
    );
  return (
    <section className="configuration-editor" key={values.epoch}>
      <h3>Exclusive gateway</h3>
      <label>
        Gateway role
        <select
          value={node.type}
          disabled={values.pending}
          onChange={(event) =>
            command({
              type: 'node.change',
              workflowId: workflow.id,
              nodeId: node.id,
              kind: event.target.value as 'exclusiveSplit' | 'exclusiveMerge',
            })
          }
        >
          <option value="exclusiveSplit">Split</option>
          <option value="exclusiveMerge">Merge</option>
        </select>
      </label>
      {split && (
        <>
          <label>
            Response source
            <select
              data-field-path={`${path}/config/sourceNodeId`}
              value={draft.sourceNodeId}
              onChange={(event) => setDraft({ ...draft, sourceNodeId: event.target.value })}
            >
              <option value="">Choose an HTTP node</option>
              {sources.map((candidate) => (
                <option key={candidate.id} value={candidate.id}>
                  {candidate.id}
                </option>
              ))}
            </select>
          </label>
          {document && (
            <RecordFields
              label="Condition inputs"
              value={draft.inputs}
              path={`${path}/config/inputs`}
              onChange={(inputs) => setDraft({ ...draft, inputs })}
              create={() => ({ source: 'literal', value: '' }) as Scalar}
            >
              {(value, change, field) => (
                <SourceEditor
                  value={value}
                  onChange={(next) => change(next as Scalar)}
                  path={field}
                  scope={{ document, workflow, nodeId: node.id }}
                />
              )}
            </RecordFields>
          )}
          {draft.routes.map((edge) => {
            const edgePath = `${path.replace(/\/nodes\/\d+$/, '')}/edges/${workflow.edges.findIndex((item) => item.id === edge.id)}`;
            return (
              <fieldset key={edge.id}>
                <legend>
                  {edge.id} → {edge.to}
                </legend>
                <label>
                  Route type
                  <select
                    value={edge.default ? 'default' : 'condition'}
                    data-field-path={edgePath}
                    onChange={(event) =>
                      setDraft({
                        ...draft,
                        routes: draft.routes.map((item) =>
                          item.id !== edge.id
                            ? item
                            : event.target.value === 'default'
                              ? { id: edge.id, from: edge.from, to: edge.to, default: true }
                              : { id: edge.id, from: edge.from, to: edge.to, condition: { expression: 'false' } }
                        ),
                      })
                    }
                  >
                    <option value="condition">Condition</option>
                    <option value="default">Default</option>
                  </select>
                </label>
                {!edge.default && (
                  <label>
                    Condition
                    <textarea
                      aria-label={`Condition ${edge.id}`}
                      data-field-path={`${edgePath}/condition/expression`}
                      value={edge.condition?.expression ?? ''}
                      placeholder="response.status = 200"
                      onChange={(event) =>
                        setDraft({
                          ...draft,
                          routes: draft.routes.map((item) =>
                            item.id === edge.id ? { ...item, condition: { expression: event.target.value } } : item
                          ),
                        })
                      }
                    />
                  </label>
                )}
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={values.pending}
                  onClick={() =>
                    command({ type: 'graph.delete', workflowId: workflow.id, nodeIds: [], edgeIds: [edge.id] })
                  }
                >
                  Remove route
                </Button>
              </fieldset>
            );
          })}
        </>
      )}
      {split && (
        <ConfigurationActions
          pending={values.pending}
          disabled={values.fragments}
          discard={values.discard}
          apply={() =>
            command({
              type: 'gateway.configure',
              workflowId: workflow.id,
              nodeId: node.id,
              sourceNodeId: draft.sourceNodeId,
              inputs: draft.inputs,
              routes: draft.routes,
            })
          }
        />
      )}
    </section>
  );
}
