import type { ScenarioDocument } from '../../lib/scenario-types';
import type { ScenarioConfiguration as Configuration } from './configuration';
import { Button } from '../../components/ui/button';
import { allocateId } from './ids';
import { useScenarioStore } from './store';
import { MappingFields } from './WorkflowConfiguration';
import {
  ConfigurationActions,
  JsonField,
  NumberField,
  RecordFields,
  TextMap,
  useConfiguration,
  type Scalar,
} from './ConfigurationFields';

type Binding = ScenarioDocument['groups'][number]['lifecycle']['iteration'];
function BindingFields({
  label,
  value,
  onChange,
  document,
  path,
  required = false,
  lifecycle = false,
}: {
  label: string;
  value: Binding | null;
  onChange(value: Binding | null): void;
  document: ScenarioDocument;
  path: string;
  required?: boolean;
  lifecycle?: boolean;
}) {
  const workflow = document.workflows.find((workflow) => workflow.id === value?.workflowId);
  return (
    <fieldset className="configuration-record">
      <legend>{label}</legend>
      <label>
        Workflow
        <select
          data-field-path={`${path}/workflowId`}
          value={value?.workflowId ?? ''}
          onChange={(event) =>
            onChange(event.target.value ? { workflowId: event.target.value, inputs: value?.inputs ?? {} } : null)
          }
        >
          <option value="">{required ? 'Choose the iteration workflow' : 'Not configured'}</option>
          {value && !workflow && <option value={value.workflowId}>Missing: {value.workflowId}</option>}
          {document.workflows.map((workflow) => (
            <option key={workflow.id} value={workflow.id}>
              {workflow.name}
            </option>
          ))}
        </select>
      </label>
      {value && (
        <MappingFields
          value={value.inputs}
          onChange={(inputs) => onChange({ ...value, inputs })}
          ports={workflow?.inputs ?? {}}
          path={`${path}/inputs`}
          scope={{ document, lifecycle }}
        />
      )}
    </fieldset>
  );
}

export function ScenarioConfiguration({ document }: { document: ScenarioDocument }) {
  const configuration: Configuration = {
    environment: document.environment,
    clientProfiles: document.clientProfiles,
    identities: document.identities,
    groups: document.groups,
  };
  const values = useConfiguration('scenario', configuration);
  const { draft, setDraft } = values;
  const scopedDocument = { ...document, ...draft };
  const group = draft.groups[0];
  const updateGroup = (next: NonNullable<typeof group>) =>
    setDraft({ ...draft, groups: [next, ...draft.groups.slice(1)] });
  const binding = (phase: 'initialize' | 'iteration' | 'cleanup', value: Binding | null) => {
    if (!group) return;
    updateGroup({
      ...group,
      lifecycle: {
        ...group.lifecycle,
        [phase]: value ?? (phase === 'iteration' ? { workflowId: '', inputs: {} } : null),
      },
    });
  };
  return (
    <section className="configuration-editor" key={values.epoch}>
      <h3>Scenario configuration</h3>

      <RecordFields
        label="Scenario variables"
        value={draft.environment.variables}
        path="/environment/variables"
        create={() => ''}
        onChange={(variables) => setDraft({ ...draft, environment: { variables } })}
      >
        {(value, change, field) => (
          <JsonField
            label="Variable JSON value"
            value={value}
            onChange={(next) => change(next as typeof value)}
            path={field}
          />
        )}
      </RecordFields>
      <h3>Client profiles</h3>
      {draft.clientProfiles.map((profile, index) => (
        <fieldset className="configuration-record" key={profile.id}>
          <legend>{profile.id}</legend>
          <label>
            Profile base URL
            <input
              data-field-path={`/clientProfiles/${index}/baseUrl`}
              value={profile.baseUrl}
              onChange={(event) =>
                setDraft({
                  ...draft,
                  clientProfiles: draft.clientProfiles.map((value, key) =>
                    key === index ? { ...value, baseUrl: event.target.value } : value
                  ),
                })
              }
            />
          </label>
          <NumberField
            label="Profile timeout (ms)"
            value={profile.timeoutMs}
            path={`/clientProfiles/${index}/timeoutMs`}
            onChange={(timeoutMs) =>
              setDraft({
                ...draft,
                clientProfiles: draft.clientProfiles.map((value, key) =>
                  key === index ? { ...value, timeoutMs: timeoutMs ?? 5000 } : value
                ),
              })
            }
          />
          <TextMap
            label="Profile headers"
            value={profile.headers}
            path={`/clientProfiles/${index}/headers`}
            onChange={(headers) =>
              setDraft({
                ...draft,
                clientProfiles: draft.clientProfiles.map((value, key) =>
                  key === index ? { ...value, headers } : value
                ),
              })
            }
          />
          <Button
            size="sm"
            variant="ghost"
            onClick={() =>
              setDraft({ ...draft, clientProfiles: draft.clientProfiles.filter((_, key) => key !== index) })
            }
          >
            Remove profile
          </Button>
        </fieldset>
      ))}
      <Button
        size="sm"
        variant="ghost"
        onClick={() =>
          setDraft({
            ...draft,
            clientProfiles: [
              ...draft.clientProfiles,
              {
                id: allocateId(
                  'client',
                  draft.clientProfiles.map((value) => value.id)
                ),
                baseUrl: 'http://localhost:8080',
                timeoutMs: 5000,
                headers: {},
              },
            ],
          })
        }
      >
        Add client profile
      </Button>
      <details>
        <summary>Identity references (optional)</summary>

        {draft.identities.map((identity, index) => (
          <fieldset key={identity.id} className="configuration-record">
            <legend>{identity.id}</legend>
            <RecordFields
              label="Credential references"
              value={identity.credentials}
              path={`/identities/${index}/credentials`}
              create={() => ({ source: 'env' as const, name: '' })}
              onChange={(credentials) =>
                setDraft({
                  ...draft,
                  identities: draft.identities.map((value, key) => (key === index ? { ...value, credentials } : value)),
                })
              }
            >
              {(reference, change, field) => (
                <label>
                  Environment variable name
                  <input
                    data-field-path={`${field}/name`}
                    value={reference.name}
                    autoComplete="off"
                    onChange={(event) => change({ source: 'env', name: event.target.value })}
                  />
                </label>
              )}
            </RecordFields>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setDraft({ ...draft, identities: draft.identities.filter((_, key) => key !== index) })}
            >
              Remove identity
            </Button>
          </fieldset>
        ))}
        <Button
          size="sm"
          variant="ghost"
          onClick={() =>
            setDraft({
              ...draft,
              identities: [
                ...draft.identities,
                {
                  id: allocateId(
                    'identity',
                    draft.identities.map((value) => value.id)
                  ),
                  credentials: {},
                },
              ],
            })
          }
        >
          Add identity references
        </Button>
      </details>
      <h3>Workload and lifecycle</h3>
      {group ? (
        <>
          <p className="muted">Workload: {group.id} · 1 user · once</p>
          <label>
            Selected client profile
            <select
              data-field-path="/groups/0/clientProfileId"
              value={group.clientProfileId}
              onChange={(event) => updateGroup({ ...group, clientProfileId: event.target.value })}
            >
              {!draft.clientProfiles.some((profile) => profile.id === group.clientProfileId) && (
                <option value={group.clientProfileId}>Missing: {group.clientProfileId}</option>
              )}
              {draft.clientProfiles.map((profile) => (
                <option key={profile.id}>{profile.id}</option>
              ))}
            </select>
          </label>
          <label>
            Optional identity assignment
            <select
              data-field-path="/groups/0/identityAssignment/identityId"
              value={group.identityAssignment.identityId ?? ''}
              onChange={(event) =>
                updateGroup({
                  ...group,
                  identityAssignment: { mode: 'shared', identityId: event.target.value || null },
                })
              }
            >
              <option value="">Anonymous / node-owned Authentication</option>
              {group.identityAssignment.identityId &&
                !draft.identities.some((identity) => identity.id === group.identityAssignment.identityId) && (
                  <option value={group.identityAssignment.identityId}>
                    Missing: {group.identityAssignment.identityId}
                  </option>
                )}
              {draft.identities.map((identity) => (
                <option key={identity.id}>{identity.id}</option>
              ))}
            </select>
          </label>
          <BindingFields
            label="Initialize once"
            document={scopedDocument}
            path="/groups/0/lifecycle/initialize"
            value={group.lifecycle.initialize}
            onChange={(value) => binding('initialize', value)}
          />
          <BindingFields
            label="Iteration once"
            document={scopedDocument}
            path="/groups/0/lifecycle/iteration"
            value={group.lifecycle.iteration}
            onChange={(value) => binding('iteration', value)}
            required
            lifecycle
          />
          <BindingFields
            label="Bounded cleanup"
            document={scopedDocument}
            path="/groups/0/lifecycle/cleanup"
            value={group.lifecycle.cleanup}
            onChange={(value) => binding('cleanup', value)}
          />
          <NumberField
            label="Cleanup timeout (ms)"
            max={5000}
            value={group.lifecycle.cleanupTimeoutMs}
            path="/groups/0/lifecycle/cleanupTimeoutMs"
            onChange={(cleanupTimeoutMs) =>
              updateGroup({ ...group, lifecycle: { ...group.lifecycle, cleanupTimeoutMs: cleanupTimeoutMs ?? 2000 } })
            }
          />
          {draft.groups.length > 1 && (
            <>
              <p className="field-error">Only one workload is executable. Remove extra workloads before running.</p>
              <Button size="sm" variant="ghost" onClick={() => setDraft({ ...draft, groups: [group] })}>
                Keep only this workload
              </Button>
            </>
          )}
        </>
      ) : (
        <Button
          size="sm"
          onClick={() =>
            setDraft({
              ...draft,
              groups: [
                {
                  id: 'group-1',
                  clientProfileId: draft.clientProfiles[0]?.id ?? '',
                  identityAssignment: { mode: 'shared', identityId: null },
                  executor: { kind: 'once' },
                  lifecycle: {
                    initialize: null,
                    iteration: { workflowId: document.workflows[0]?.id ?? '', inputs: {} as Record<string, Scalar> },
                    cleanup: null,
                    cleanupTimeoutMs: 2000,
                  },
                },
              ],
            })
          }
        >
          Add workload
        </Button>
      )}
      <ConfigurationActions
        pending={values.pending || values.fragments}
        disabled={values.fragments}
        discard={values.discard}
        apply={() => useScenarioStore.getState().command({ type: 'scenario.configure', configuration: draft })}
      />
    </section>
  );
}

export function ScenarioOverview({ document }: { document: ScenarioDocument }) {
  return (
    <section className="configuration-overview">
      <h3>Transport and lifecycle</h3>
      <p className="muted">Scenario variables: {Object.keys(document.environment.variables).join(', ') || 'None'}</p>
      {document.clientProfiles.map((profile) => (
        <dl className="details" key={profile.id}>
          <div>
            <dt>Profile</dt>
            <dd>{profile.id}</dd>
          </div>
          <div>
            <dt>Base URL</dt>
            <dd>{profile.baseUrl}</dd>
          </div>
          <div>
            <dt>Timeout</dt>
            <dd>{profile.timeoutMs} ms</dd>
          </div>
          <div>
            <dt>Header names</dt>
            <dd>{Object.keys(profile.headers).join(', ') || 'None'}</dd>
          </div>
        </dl>
      ))}
      {document.groups.map((group) => (
        <dl className="details" key={group.id}>
          <div>
            <dt>Workload</dt>
            <dd>{group.id} · one user · once</dd>
          </div>
          <div>
            <dt>Client profile</dt>
            <dd>{group.clientProfileId}</dd>
          </div>
          {(['initialize', 'iteration', 'cleanup'] as const).map((phase) => (
            <div key={phase}>
              <dt>{phase}</dt>
              <dd>{group.lifecycle[phase]?.workflowId ?? 'Not configured'}</dd>
            </div>
          ))}
          <div>
            <dt>Cleanup deadline</dt>
            <dd>{group.lifecycle.cleanupTimeoutMs} ms</dd>
          </div>
        </dl>
      ))}
    </section>
  );
}
