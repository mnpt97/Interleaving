import { extractionOutputs } from './extraction';
import { useEffect, useId, useState, type ReactNode } from 'react';
import type { z } from 'zod';
import { Plus, Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/button';
import { templateValueSchema, type ScenarioDocument, type WorkflowDefinition } from '../../lib/scenario-types';
import { useScenarioStore } from './store';

export type Template = z.infer<typeof templateValueSchema>;
export type Scalar = WorkflowDefinition['outputs'][string]['value'];
export type Scope = { document: ScenarioDocument; workflow?: WorkflowDefinition; nodeId?: string; lifecycle?: boolean };
let rowSequence = 0;
const rowKey = () => `configuration-row-${rowSequence++}`;
export const pointer = (value: string) => value.replace(/~/g, '~0').replace(/\//g, '~1');

export function useFragment(key: string, error: string | null) {
  const owner = useId();
  useEffect(() => {
    useScenarioStore.getState().setFormError(`fragment:${key}:${owner}`, error);
    return () => useScenarioStore.getState().setFormError(`fragment:${key}:${owner}`, null);
  }, [key, error, owner]);
}

/** Keep incomplete local fields outside the executable document until Apply. */
export function useConfiguration<T>(key: string, value: T) {
  const baseline = JSON.stringify(value);
  const [draft, setDraft] = useState(value);
  const [epoch, setEpoch] = useState(0);
  useEffect(() => {
    setDraft(JSON.parse(baseline));
    setEpoch((value) => value + 1);
  }, [baseline]);
  const pending = JSON.stringify(draft) !== baseline;
  useEffect(() => {
    useScenarioStore
      .getState()
      .setFormError(`configuration:${key}`, pending ? 'Apply or discard these configuration changes.' : null);
    return () => useScenarioStore.getState().setFormError(`configuration:${key}`, null);
  }, [key, pending]);
  const fragments = useScenarioStore((state) =>
    Object.keys(state.formErrors).some((key) => key.startsWith('fragment:'))
  );
  return {
    draft,
    setDraft,
    pending,
    fragments,
    epoch,
    discard: () => {
      setDraft(JSON.parse(baseline));
      setEpoch((value) => value + 1);
    },
  };
}

export function ConfigurationActions({
  pending,
  disabled,
  apply,
  discard,
}: {
  pending: boolean;
  disabled?: boolean;
  apply(): void;
  discard(): void;
}) {
  return pending ? (
    <div className="configuration-actions">
      <Button size="sm" disabled={disabled} onClick={apply}>
        Apply configuration
      </Button>
      <Button size="sm" variant="ghost" onClick={discard}>
        Discard fields
      </Button>
    </div>
  ) : null;
}

export function JsonField({
  label,
  value,
  path,
  onChange,
  scalar = false,
}: {
  label: string;
  value: unknown;
  path: string;
  scalar?: boolean;
  onChange(value: unknown): void;
}) {
  const baseline = JSON.stringify(value ?? null, null, 2);
  const [text, setText] = useState(baseline);
  const messageId = useId();
  useEffect(() => setText(baseline), [baseline]);
  let error: string | null = null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    error = 'Enter valid JSON. Incomplete text stays local until corrected.';
  }
  useFragment(path, error);
  return (
    <label>
      {label}
      <textarea
        aria-label={label}
        aria-describedby={
          error || (scalar && !['string', 'number', 'boolean'].includes(typeof parsed)) ? messageId : undefined
        }
        data-field-path={path}
        value={text}
        aria-invalid={Boolean(error)}
        rows={Math.min(12, Math.max(3, text.split('\n').length))}
        onChange={(event) => {
          setText(event.target.value);
          try {
            const next: unknown = JSON.parse(event.target.value);
            onChange(next);
          } catch {
            /* Preserve incomplete text locally. */
          }
        }}
      />
      {scalar && !error && !['string', 'number', 'boolean'].includes(typeof parsed) && (
        <span className="muted" id={messageId}>
          Use a string, integer, or boolean.
        </span>
      )}
      {error && (
        <span className="field-error" role="alert" id={messageId}>
          {error}
        </span>
      )}
    </label>
  );
}

/** Named rows retain duplicate/unfinished names locally instead of overwriting data. */
export function RecordFields<T>({
  label,
  value,
  path,
  create,
  onChange,
  children,
  nameOptions,
}: {
  nameOptions?: string[];
  label: string;
  value: Record<string, T>;
  path: string;
  create(): T;
  onChange(value: Record<string, T>): void;
  children(value: T, change: (value: T) => void, path: string): ReactNode;
}) {
  const baseline = JSON.stringify(value);
  const [rows, setRows] = useState(Object.entries(value));
  const [keys, setKeys] = useState(() => Object.keys(value).map(rowKey));
  useEffect(() => {
    const next = Object.entries(JSON.parse(baseline) as Record<string, T>);
    setRows(next);
    setKeys((keys) => next.map((_, index) => keys[index] ?? rowKey()));
  }, [baseline]);
  const names = rows.map(([name]) => name);
  const invalid = names.some((name) => !name.trim()) || new Set(names).size !== names.length;
  const localNames = JSON.stringify(rows) !== JSON.stringify(Object.entries(JSON.parse(baseline)));
  useFragment(path, invalid && localNames ? 'Complete each name and use unique names before applying.' : null);
  const change = (next: [string, T][]) => {
    setRows(next);
    const keys = next.map(([name]) => name);
    if (keys.every((name) => name.trim()) && new Set(keys).size === keys.length) onChange(Object.fromEntries(next));
  };
  return (
    <fieldset className="configuration-record" data-field-path={path}>
      <legend>{label}</legend>
      {rows.map(([name, item], index) => (
        <div className="configuration-row" key={keys[index]}>
          <div className="configuration-row-heading">
            <label>
              Name
              {nameOptions ? (
                <select
                  aria-label={`${label} name ${index + 1}`}
                  value={name}
                  data-field-path={`${path}/${pointer(name)}`}
                  onChange={(event) =>
                    change(rows.map((row, key) => (key === index ? [event.target.value, item] : row)))
                  }
                >
                  <option value="">Choose an input port</option>
                  {name && !nameOptions.includes(name) && <option value={name}>Missing: {name}</option>}
                  {nameOptions.map((name) => (
                    <option key={name}>{name}</option>
                  ))}
                </select>
              ) : (
                <input
                  aria-label={`${label} name ${index + 1}`}
                  value={name}
                  data-field-path={`${path}/${pointer(name)}`}
                  onChange={(event) =>
                    change(rows.map((row, key) => (key === index ? [event.target.value, item] : row)))
                  }
                />
              )}
            </label>
            <Button
              size="icon"
              variant="ghost"
              aria-label={`Remove ${label} row ${index + 1}`}
              type="button"
              onClick={() => {
                setKeys((keys) => keys.filter((_, key) => key !== index));
                change(rows.filter((_, key) => key !== index));
              }}
            >
              <Trash2 />
            </Button>
          </div>
          {children(
            item,
            (next) => change(rows.map((row, key) => (key === index ? [name, next] : row))),
            `${path}/${pointer(name)}`
          )}
        </div>
      ))}
      {invalid && (
        <p role="alert" className="field-error">
          Complete each name and use unique names.
        </p>
      )}
      <Button
        variant="ghost"
        size="sm"
        type="button"
        onClick={() => {
          setKeys((keys) => [...keys, rowKey()]);
          change([...rows, ['', create()]]);
        }}
      >
        <Plus />
        Add {label.toLowerCase()}
      </Button>
    </fieldset>
  );
}

export function TextMap({
  label,
  value,
  path,
  onChange,
}: {
  label: string;
  value: Record<string, string>;
  path: string;
  onChange(value: Record<string, string>): void;
}) {
  return (
    <RecordFields label={label} value={value} path={path} create={() => ''} onChange={onChange}>
      {(item, change, field) => (
        <label>
          Value
          <input data-field-path={field} value={item} onChange={(event) => change(event.target.value)} />
        </label>
      )}
    </RecordFields>
  );
}

export function earlierNodes(scope: Scope) {
  const workflow = scope.workflow;
  if (!workflow) return [];
  const consumer = scope.nodeId ?? workflow.nodes.find((node) => node.type === 'end')?.id;
  if (!consumer) return [];
  // A value must be available on every route, not merely on one ancestor path.
  return workflow.nodes.filter((candidate) => {
    if (candidate.id === consumer) return false;
    const pending = workflow.nodes.filter((node) => node.type === 'start').map((node) => node.id);
    const seen = new Set<string>();
    while (pending.length) {
      const id = pending.pop()!;
      if (id === candidate.id || seen.has(id)) continue;
      if (id === consumer) return false;
      seen.add(id);
      pending.push(...workflow.edges.filter((edge) => edge.from === id).map((edge) => edge.to));
    }
    return true;
  });
}

function Choice({
  label,
  value,
  names,
  path,
  onChange,
}: {
  label: string;
  value: string;
  names: string[];
  path: string;
  onChange(value: string): void;
}) {
  return (
    <label>
      {label}
      <select data-field-path={path} value={value} onChange={(event) => onChange(event.target.value)}>
        <option value="">Choose a reference</option>
        {value && !names.includes(value) && <option value={value}>Missing: {value}</option>}
        {names.map((name) => (
          <option key={name} value={name}>
            {name}
          </option>
        ))}
      </select>
    </label>
  );
}

export function SourceEditor({
  value,
  onChange,
  path,
  scope,
  stringOnly = false,
  template = false,
  depth = 0,
  showSource = true,
}: {
  value: Scalar | Template;
  onChange(value: Scalar | Template): void;
  path: string;
  scope: Scope;
  stringOnly?: boolean;
  template?: boolean;
  depth?: number;
  showSource?: boolean;
}) {
  const candidates = earlierNodes(scope);
  const types = [
    'literal',
    'environment',
    'identity',
    'input',
    'extraction',
    'callOutput',
    ...(!stringOnly ? ['response'] : []),
    ...(scope.lifecycle ? ['lifecycle'] : []),
    ...(template && depth < 31 ? ['object', 'array'] : []),
  ];
  const defaults: Record<string, Scalar | Template> = {
    literal: { source: 'literal', value: '' },
    environment: { source: 'environment', name: '' },
    identity: { source: 'identity', field: '' },
    input: { source: 'input', name: '' },
    extraction: { source: 'extraction', nodeId: '', name: '' },
    callOutput: { source: 'callOutput', nodeId: '', name: '' },
    response: { source: 'response', nodeId: '', field: 'status' },
    lifecycle: { source: 'lifecycle', phase: 'initialize', name: '' },
    object: { source: 'object', fields: {} },
    array: { source: 'array', items: [] },
  };
  const names: Record<string, string> = {
    literal: 'Literal',
    environment: 'Scenario variable',
    identity: 'Assigned identity field',
    input: 'Workflow input',
    extraction: 'Extract output',
    callOutput: 'Call output',
    response: 'HTTP response status',
    lifecycle: 'Initialize output',
    object: 'Object',
    array: 'Array',
  };
  const identity = scope.document.identities.find(
    (identity) => identity.id === scope.document.groups[0]?.identityAssignment.identityId
  );
  const init = scope.document.workflows.find(
    (workflow) => workflow.id === scope.document.groups[0]?.lifecycle.initialize?.workflowId
  );
  const referenceChoices = candidates.flatMap((node) =>
    value.source === 'extraction' && node.type === 'extract'
      ? Object.entries(extractionOutputs(node.config))
          .filter(([, output]) => !stringOnly || output.type === 'string')
          .map(([name]) => ({ nodeId: node.id, name }))
      : value.source === 'callOutput' && node.type === 'call'
        ? Object.keys(
            scope.document.workflows.find((workflow) => workflow.id === node.config.workflowId)?.outputs ?? {}
          ).map((name) => ({ nodeId: node.id, name }))
        : []
  );
  const selected = 'nodeId' in value && 'name' in value ? JSON.stringify([value.nodeId, value.name]) : '';
  return (
    <div className="source-editor" data-field-path={path}>
      {showSource && (
        <label>
          Source
          <select value={value.source} onChange={(event) => onChange(defaults[event.target.value])}>
            {!types.includes(value.source) && <option value={value.source}>{names[value.source]}</option>}
            {types.map((type) => (
              <option key={type} value={type}>
                {names[type]}
              </option>
            ))}
          </select>
        </label>
      )}
      {value.source === 'literal' &&
        (stringOnly ? (
          <label>
            Literal string
            <input
              value={typeof value.value === 'string' ? value.value : ''}
              onChange={(event) => onChange({ source: 'literal', value: event.target.value })}
            />
          </label>
        ) : (
          <JsonField
            label="Literal JSON value"
            value={value.value}
            scalar={!template}
            path={`${path}/value`}
            onChange={(next) =>
              onChange({ source: 'literal', value: next as Extract<Template, { source: 'literal' }>['value'] })
            }
          />
        ))}
      {value.source === 'environment' && (
        <Choice
          label="Scenario variable"
          value={value.name}
          names={Object.keys(scope.document.environment.variables)}
          path={`${path}/name`}
          onChange={(name) => onChange({ ...value, name })}
        />
      )}
      {value.source === 'identity' && (
        <Choice
          label="Identity field"
          value={value.field}
          names={Object.keys(identity?.credentials ?? {})}
          path={`${path}/field`}
          onChange={(field) => onChange({ ...value, field })}
        />
      )}
      {value.source === 'input' && (
        <Choice
          label="Input name"
          value={value.name}
          names={Object.keys(scope.workflow?.inputs ?? {})}
          path={`${path}/name`}
          onChange={(name) => onChange({ ...value, name })}
        />
      )}
      {value.source === 'lifecycle' && (
        <Choice
          label="Initialize output"
          value={value.name}
          names={Object.keys(init?.outputs ?? {})}
          path={`${path}/name`}
          onChange={(name) => onChange({ ...value, name })}
        />
      )}
      {(value.source === 'extraction' || value.source === 'callOutput') && (
        <label>
          Output reference
          <select
            data-field-path={`${path}/nodeId`}
            value={selected}
            onChange={(event) => {
              const [nodeId, name] = event.target.value ? (JSON.parse(event.target.value) as string[]) : ['', ''];
              onChange({ ...value, nodeId, name });
            }}
          >
            <option value="">Choose an output</option>
            {!referenceChoices.some((item) => JSON.stringify([item.nodeId, item.name]) === selected) && (
              <option value={selected}>
                Missing: {value.nodeId}.{value.name}
              </option>
            )}
            {referenceChoices.map((item) => (
              <option key={JSON.stringify([item.nodeId, item.name])} value={JSON.stringify([item.nodeId, item.name])}>
                {item.nodeId}.{item.name}
              </option>
            ))}
          </select>
        </label>
      )}
      {value.source === 'response' && (
        <Choice
          label="HTTP request"
          value={value.nodeId}
          names={candidates.filter((node) => node.type === 'http').map((node) => node.id)}
          path={`${path}/nodeId`}
          onChange={(nodeId) => onChange({ ...value, nodeId })}
        />
      )}
      {value.source === 'object' && (
        <RecordFields
          label="Object fields"
          value={value.fields}
          path={`${path}/fields`}
          create={() => ({ source: 'literal', value: '' }) as Template}
          onChange={(fields) => onChange({ ...value, fields })}
        >
          {(item, change, field) => (
            <SourceEditor
              value={item}
              path={field}
              scope={scope}
              template
              depth={depth + 1}
              onChange={(next) => change(next as Template)}
            />
          )}
        </RecordFields>
      )}
      {value.source === 'array' && (
        <TemplateArray value={value} onChange={onChange} path={path} scope={scope} depth={depth} />
      )}
    </div>
  );
}

export function NumberField({
  label,
  value,
  path,
  onChange,
  optional = false,
  max = 300000,
}: {
  label: string;
  value: number | null;
  path: string;
  optional?: boolean;
  max?: number;
  onChange(value: number | null): void;
}) {
  const messageId = useId();
  const [text, setText] = useState(value === null ? '' : String(value));
  useEffect(() => setText(value === null ? '' : String(value)), [value]);
  const valid =
    (optional && text === '') ||
    (text.trim() !== '' && Number.isSafeInteger(Number(text)) && Number(text) > 0 && Number(text) <= max);
  useFragment(
    path,
    valid || text === (value === null ? '' : String(value)) ? null : `Enter an integer from 1 to ${max}.`
  );
  return (
    <label>
      {label}
      <input
        aria-label={label}
        aria-describedby={!valid ? messageId : undefined}
        data-field-path={path}
        type="number"
        value={text}
        aria-invalid={!valid}
        placeholder={optional ? 'Profile default' : undefined}
        onChange={(event) => {
          const next = event.target.value;
          setText(next);
          if (optional && next === '') onChange(null);
          else if (next.trim() && Number.isSafeInteger(Number(next)) && Number(next) > 0 && Number(next) <= max)
            onChange(Number(next));
        }}
      />
      {!valid && (
        <span className="field-error" role="alert" id={messageId}>
          Enter an integer from 1 to {max}.
        </span>
      )}
    </label>
  );
}

function TemplateArray({
  value,
  onChange,
  path,
  scope,
  depth,
}: {
  value: Extract<Template, { source: 'array' }>;
  onChange(value: Template): void;
  path: string;
  scope: Scope;
  depth: number;
}) {
  const [keys, setKeys] = useState(() => value.items.map(rowKey));
  useEffect(() => setKeys((keys) => value.items.map((_, index) => keys[index] ?? rowKey())), [value.items.length]);
  return (
    <div>
      {value.items.map((item, index) => (
        <fieldset key={keys[index]}>
          <legend>Item {index + 1}</legend>
          <SourceEditor
            value={item}
            path={`${path}/items/${index}`}
            scope={scope}
            template
            depth={depth + 1}
            onChange={(next) =>
              onChange({ ...value, items: value.items.map((item, key) => (key === index ? (next as Template) : item)) })
            }
          />
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              setKeys((keys) => keys.filter((_, key) => key !== index));
              onChange({ ...value, items: value.items.filter((_, key) => key !== index) });
            }}
          >
            Remove item
          </Button>
        </fieldset>
      ))}
      <Button
        size="sm"
        variant="ghost"
        onClick={() => {
          setKeys((keys) => [...keys, rowKey()]);
          onChange({ ...value, items: [...value.items, { source: 'literal', value: '' }] });
        }}
      >
        Add array item
      </Button>
    </div>
  );
}
