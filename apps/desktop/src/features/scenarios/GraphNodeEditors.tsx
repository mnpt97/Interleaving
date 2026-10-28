import { extractionOutputs } from './extraction';
import { SourceEditor, earlierNodes, RecordFields, useConfiguration, type Scalar } from './ConfigurationFields';
import { sessionHeaderSchema } from '../../lib/scenario-types';
import type { z } from 'zod';
import { X } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { WorkflowDefinition, WorkflowNode } from '../../lib/scenario-types';
import { Button } from '../../components/ui/button';
import { useScenarioStore } from './store';

function usePending(key: string, pending: boolean) {
  useEffect(() => {
    useScenarioStore.getState().setFormError(key, pending ? 'Apply or discard these configuration fields.' : null);
    return () => useScenarioStore.getState().setFormError(key, null);
  }, [key, pending]);
}

export function ExtractEditor({
  node,
  workflow,
}: {
  node: Extract<WorkflowNode, { type: 'extract' }>;
  workflow: WorkflowDefinition;
}) {
  const document = useScenarioStore((state) => state.document)!;
  const fieldPath = `/workflows/${document.workflows.findIndex((value) => value.id === workflow.id)}/nodes/${workflow.nodes.findIndex((value) => value.id === node.id)}/config`;
  const normalized = { sourceNodeId: node.config.sourceNodeId, outputs: extractionOutputs(node.config) };
  const {
    draft: config,
    setDraft: setConfig,
    pending,
    fragments,
    epoch,
    discard,
  } = useConfiguration(`extract:${workflow.id}:${node.id}`, normalized);
  const entries = Object.entries(config.outputs);
  const valid =
    Boolean(config.sourceNodeId) &&
    entries.length > 0 &&
    entries.length <= 32 &&
    entries.every(([name, value]) => /^[A-Za-z_][A-Za-z0-9_]{0,127}$/.test(name) && value.expression.trim());
  return (
    <section className="graph-node-editor">
      <h3>Extract response properties</h3>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (valid && !fragments)
            useScenarioStore
              .getState()
              .command({ type: 'node.put', workflowId: workflow.id, node: { ...node, config } });
        }}
      >
        <label>
          Source HTTP request
          <select
            data-field-path={`${fieldPath}/sourceNodeId`}
            value={config.sourceNodeId}
            onChange={(event) => setConfig({ ...config, sourceNodeId: event.target.value })}
          >
            <option value="">Choose a request</option>
            {workflow.nodes
              .filter((candidate) => candidate.type === 'http')
              .map((candidate) => (
                <option key={candidate.id} value={candidate.id}>
                  {candidate.name || candidate.id}
                </option>
              ))}
          </select>
        </label>
        <RecordFields
          key={epoch}
          label="Outputs"
          path={`${fieldPath}/outputs`}
          value={config.outputs}
          create={() => ({ expression: 'response.body.', type: 'string' as const })}
          onChange={(outputs) => setConfig({ ...config, outputs })}
        >
          {(output, change, path) => (
            <>
              <label>
                Property expression
                <input
                  data-field-path={`${path}/expression`}
                  value={output.expression}
                  onChange={(event) => change({ ...output, expression: event.target.value })}
                />
              </label>
              <label>
                Output type
                <select
                  data-field-path={`${path}/type`}
                  value={output.type}
                  onChange={(event) => change({ ...output, type: event.target.value as typeof output.type })}
                >
                  <option value="string">String</option>
                  <option value="integer">Integer</option>
                  <option value="boolean">Boolean</option>
                </select>
              </label>
            </>
          )}
        </RecordFields>
        {(pending || fragments) && (
          <div className="panel-actions">
            <Button size="sm" type="submit" disabled={!valid || fragments}>
              Apply extraction
            </Button>
            <Button size="sm" variant="ghost" type="button" onClick={discard}>
              Discard fields
            </Button>
          </div>
        )}
      </form>
    </section>
  );
}

export function SessionEditor({
  node,
  workflow,
}: {
  node: Extract<WorkflowNode, { type: 'setSession' }>;
  workflow: WorkflowDefinition;
}) {
  const document = useScenarioStore((state) => state.document)!;
  const fieldPath = `/workflows/${document.workflows.findIndex((value) => value.id === workflow.id)}/nodes/${workflow.nodes.findIndex((value) => value.id === node.id)}/config`;
  const [advanced, setAdvanced] = useState<Scalar>({ source: 'literal', value: '' });
  const candidates = earlierNodes({ document, workflow, nodeId: node.id });
  const [operation, setOperation] = useState('set');
  const [name, setName] = useState('Authorization');
  const [source, setSource] = useState('environment');
  const [reference, setReference] = useState('AUTH_TOKEN');
  const [prefix, setPrefix] = useState('Bearer ');
  const [pending, setPending] = useState(false);
  const focused = useScenarioStore((state) => state.focusedFieldPath);
  useEffect(() => {
    const prefixPath = `${fieldPath}/setHeaders/`;
    if (!focused?.startsWith(prefixPath)) return;
    const key = focused.slice(prefixPath.length).split('/')[0].replace(/~1/g, '/').replace(/~0/g, '~');
    const value = node.config.setHeaders[key];
    if (!value) return;
    setOperation('set');
    setName(key);
    setSource(value.source);
    setPrefix(value.prefix ?? '');
    setReference(value.source === 'environment' ? value.name : value.source === 'extraction' ? value.nodeId : '');
    const { prefix: _prefix, ...source } = value;
    setAdvanced(source as Scalar);
  }, [focused, fieldPath]);
  usePending(`session:${workflow.id}:${node.id}`, pending);
  const validHeader = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(name) && !/[\r\n]/.test(prefix);
  const command = (config: typeof node.config) =>
    useScenarioStore.getState().command({ type: 'node.put', workflowId: workflow.id, node: { ...node, config } });
  const reset = () => {
    setPending(false);
    setName('Authorization');
    setReference('AUTH_TOKEN');
    setPrefix('Bearer ');
    setSource('environment');
    setOperation('set');
  };
  return (
    <section className="graph-node-editor">
      <h3>Session headers</h3>
      <ul className="session-operations">
        {Object.entries(node.config.setHeaders).map(([key, value]) => (
          <li key={`set:${key}`}>
            <span>
              Set {key} · {value.source}
            </span>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                setOperation('set');
                setName(key);
                setSource(value.source);
                setPrefix(value.prefix ?? '');
                setReference(
                  value.source === 'environment' ? value.name : value.source === 'extraction' ? value.nodeId : ''
                );
                const { prefix: _prefix, ...source } = value;
                setAdvanced(source as Scalar);
                setPending(true);
              }}
            >
              Edit
            </Button>
            <Button
              size="icon"
              variant="ghost"
              aria-label={`Remove set operation ${key}`}
              onClick={() => {
                const setHeaders = { ...node.config.setHeaders };
                delete setHeaders[key];
                command({ ...node.config, setHeaders });
              }}
            >
              <X />
            </Button>
          </li>
        ))}
        {node.config.removeHeaders.map((key) => (
          <li key={`remove:${key}`}>
            <span>Remove {key}</span>
            <Button
              size="icon"
              variant="ghost"
              aria-label={`Remove removal operation ${key}`}
              onClick={() =>
                command({ ...node.config, removeHeaders: node.config.removeHeaders.filter((name) => name !== key) })
              }
            >
              <X />
            </Button>
          </li>
        ))}
      </ul>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (
            !/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(name) ||
            /[\r\n]/.test(prefix) ||
            (operation === 'set' && ['environment', 'extraction'].includes(source) && !reference.trim())
          )
            return;
          const setHeaders = Object.fromEntries(
            Object.entries(node.config.setHeaders).filter(([key]) => key.toLowerCase() !== name.toLowerCase())
          ) as typeof node.config.setHeaders;
          const removeHeaders = node.config.removeHeaders.filter((key) => key.toLowerCase() !== name.toLowerCase());
          if (operation === 'remove') removeHeaders.push(name);
          else if (source === 'environment') setHeaders[name] = { source: 'environment', name: reference, prefix };
          else if (source === 'extraction') {
            const selected = candidates
              .flatMap((candidate) =>
                candidate.type === 'extract'
                  ? Object.entries(extractionOutputs(candidate.config))
                      .filter(([, output]) => output.type === 'string')
                      .map(([name]) => ({ nodeId: candidate.id, name }))
                  : []
              )
              .find((output) => JSON.stringify([output.nodeId, output.name]) === reference);
            if (!selected) return;
            setHeaders[name] = { source: 'extraction', ...selected, prefix };
          } else setHeaders[name] = { ...advanced, prefix } as z.infer<typeof sessionHeaderSchema>;
          command({ setHeaders, removeHeaders });
          if (!useScenarioStore.getState().error) reset();
        }}
        onChange={() => setPending(true)}
      >
        <label>
          Operation
          <select value={operation} onChange={(event) => setOperation(event.target.value)}>
            <option value="set">Set header</option>
            <option value="remove">Remove header</option>
          </select>
        </label>
        <label>
          Header name
          <input
            data-field-path={`${fieldPath}/setHeaders/${name}`}
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        {operation === 'set' && (
          <>
            <label>
              Value source
              <select
                value={source}
                onChange={(event) => {
                  setSource(event.target.value);
                  setReference('');
                  setAdvanced(
                    event.target.value === 'identity'
                      ? { source: 'identity', field: '' }
                      : event.target.value === 'input'
                        ? { source: 'input', name: '' }
                        : event.target.value === 'callOutput'
                          ? { source: 'callOutput', nodeId: '', name: '' }
                          : { source: 'literal', value: '' }
                  );
                }}
              >
                <option value="environment">Scenario variable</option>
                <option value="extraction">Extract output</option>
                <option value="input">Workflow input</option>
                <option value="identity">Assigned identity field</option>
                <option value="callOutput">Call output</option>
                <option value="literal">Literal string</option>
              </select>
            </label>
            {source === 'environment' ? (
              <label>
                Scenario variable name
                <input
                  data-field-path={`${fieldPath}/setHeaders/${name}/name`}
                  value={reference}
                  onChange={(event) => setReference(event.target.value)}
                />
              </label>
            ) : source === 'extraction' ? (
              <label>
                Extract output
                <select
                  data-field-path={`${fieldPath}/setHeaders/${name}/nodeId`}
                  value={reference}
                  onChange={(event) => setReference(event.target.value)}
                >
                  <option value="">Choose an output</option>
                  {candidates.flatMap((candidate) =>
                    candidate.type === 'extract'
                      ? Object.entries(extractionOutputs(candidate.config))
                          .filter(([, output]) => output.type === 'string')
                          .map(([name]) => (
                            <option key={`${candidate.id}::${name}`} value={JSON.stringify([candidate.id, name])}>
                              {candidate.id}.{name}
                            </option>
                          ))
                      : []
                  )}
                </select>
              </label>
            ) : (
              <SourceEditor
                value={advanced}
                onChange={(value) => setAdvanced(value as Scalar)}
                scope={{ document, workflow, nodeId: node.id }}
                path={`${fieldPath}/setHeaders/${name}`}
                stringOnly
                showSource={false}
              />
            )}
            <label>
              Prefix
              <input
                data-field-path={`${fieldPath}/setHeaders/${name}/prefix`}
                value={prefix}
                onChange={(event) => setPrefix(event.target.value)}
              />
            </label>
          </>
        )}

        {pending && !validHeader && (
          <p className="field-error" role="alert">
            Use an HTTP header name without spaces and a prefix without line breaks.
          </p>
        )}
        <div className="panel-actions">
          <Button
            size="sm"
            type="submit"
            disabled={
              !validHeader ||
              (operation === 'set' && ['environment', 'extraction'].includes(source) && !reference.trim())
            }
          >
            Apply header operation
          </Button>
          {pending && (
            <Button size="sm" variant="ghost" type="button" onClick={reset}>
              Discard fields
            </Button>
          )}
        </div>
      </form>
    </section>
  );
}
