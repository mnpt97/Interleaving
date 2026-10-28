import { AddNodePopover } from './AddNodePopover';
import { ownsPhase, phaseName } from './lifecycle';
import { useRunView } from '../runs/useRunView';
import { RunInspector } from '../runs/RunInspector';
import { GatewayConfiguration } from './GatewayConfiguration';
import { NodeIdentity } from './NodeIdentity';
import { roundPosition } from './adapters';
import { HttpConfiguration } from './HttpConfiguration';
import { WorkflowConfiguration, CallConfiguration } from './WorkflowConfiguration';
import { ScenarioConfiguration, ScenarioOverview } from './ScenarioConfiguration';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../../components/ui/tabs';
import { FieldDiagnostics } from './FieldDiagnostics';
import { ExtractEditor, SessionEditor } from './GraphNodeEditors';
import { effectiveUrl, NodeBaseUrl } from './targets';
import { selectedWorkflow } from '../../lib/scenario-types';
import { authenticationDefinition } from './authentication';
import { AuthenticationEditor } from './AuthenticationEditor';
import { AuthDetails } from './AuthDetails';
import { presentationFor, nodeName } from './node-presentation';
import { useEffect, useState } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { z } from 'zod';
import { Button } from '../../components/ui/button';
import { useScenarioStore } from './store';

const positionSchema = z.object({ x: z.number().finite(), y: z.number().finite() });
type Coordinates = z.infer<typeof positionSchema>;

function NodeContent() {
  const draft = useScenarioStore((s) => s.document);
  const mode = useScenarioStore((s) => s.mode);
  const document = draft;
  const workflowId = useScenarioStore((s) => s.selectedWorkflowId);
  const selectedId = useScenarioStore((s) => s.selectedId);
  const move = useScenarioStore((s) => s.move);
  const workflow = document ? selectedWorkflow(document, workflowId) : undefined;
  const index = workflow?.nodes.findIndex((node) => node.id === selectedId) ?? -1;
  const node = workflow?.nodes[index];
  const nodePath = `/workflows/${document?.workflows.indexOf(workflow!)}/nodes/${index}`;
  const position = selectedId ? workflow?.layout?.positions[selectedId] : undefined;
  const { x, y } = roundPosition({ x: position?.x ?? 80 + index * 260, y: position?.y ?? 160 });
  const {
    register,
    handleSubmit,
    reset,
    formState: { errors, isDirty },
  } = useForm<Coordinates>({ resolver: zodResolver(positionSchema), defaultValues: { x, y } });
  useEffect(() => {
    reset({ x, y });
  }, [selectedId, x, y, reset, mode]);
  useEffect(() => {
    useScenarioStore
      .getState()
      .setFormError(
        'position',
        mode === 'edit' && isDirty
          ? Object.keys(errors).length
            ? 'Enter finite coordinates.'
            : 'Apply the position change before saving.'
          : null
      );
    return () => useScenarioStore.getState().setFormError('position', null);
  }, [errors, isDirty, mode]);
  if (!node)
    return (
      <section>
        <p className="muted inspector-empty">Select a node.</p>
      </section>
    );
  return (
    <section>
      <div className="inspector-content">
        <h3>{nodeName(node, document)}</h3>

        <FieldDiagnostics prefix={nodePath} />
        {mode === 'edit' && workflow && (
          <NodeIdentity key={`identity:${workflow.id}:${node.id}`} workflow={workflow} nodeId={node.id} />
        )}
        <dl className="details">
          {mode === 'view' && (
            <div>
              <dt>Node ID</dt>
              <dd>
                <code>{node.id}</code>
              </dd>
            </div>
          )}
          <div>
            <dt>Type</dt>
            <dd>{presentationFor(node, document).label}</dd>
          </div>
          {node.type === 'http' && (
            <>
              <div>
                <dt>Method</dt>
                <dd>
                  <code>{node.config.method}</code>
                </dd>
              </div>
              <div>
                <dt>Path</dt>
                <dd>
                  <code>{node.config.path}</code>
                </dd>
              </div>
              <div>
                <dt>Timeout</dt>
                <dd>{node.config.timeoutMs ? `${node.config.timeoutMs} ms` : 'Profile default'}</dd>
              </div>
            </>
          )}
        </dl>
        {document && workflow && node.type === 'call' && authenticationDefinition(document, node) && (
          <AuthenticationEditor
            key={`${workflow.id}:${node.id}`}
            document={document}
            workflowId={workflow.id}
            node={node}
            definition={authenticationDefinition(document, node)!}
            editing={mode === 'edit'}
            showName={false}
          />
        )}
        {document && workflow && node.type === 'authentication' && (
          <AuthenticationEditor
            key={`${workflow.id}:${node.id}`}
            document={document}
            workflowId={workflow.id}
            node={node}
            editing={mode === 'edit'}
            showName={false}
          />
        )}
        {document && node.type === 'call' && mode === 'view' && (
          <section className="auth-details">
            <h3>Called workflow</h3>
            <p>
              {document.workflows.find((workflow) => workflow.id === node.config.workflowId)?.name ??
                node.config.workflowId}
            </p>
            <ul>
              {Object.entries(node.config.inputs).map(([name, source]) => (
                <li key={name}>
                  {name}: {source.source}
                  {'nodeId' in source ? ` · ${source.nodeId}` : ''}
                  {'name' in source ? ` · ${source.name}` : ''}
                </li>
              ))}
            </ul>
          </section>
        )}
        {document && node.type === 'http' && (
          <p className="muted">
            Effective URL: <code>{effectiveUrl(document, node.config.baseUrl, node.config.path)}</code>
          </p>
        )}
        {workflow && (node.type === 'exclusiveSplit' || node.type === 'exclusiveMerge') && (
          <GatewayConfiguration
            key={`${workflow.id}:${node.id}`}
            node={node}
            workflow={workflow}
            editing={mode === 'edit'}
            path={nodePath}
          />
        )}
        {document && <AuthDetails node={node} document={document} />}
        {mode === 'edit' && node.type === 'http' && workflow && (
          <HttpEditor key={`${workflow.id}:${node.id}`} node={node} workflowId={workflow.id} />
        )}
        {mode === 'edit' && document && workflow && node.type === 'http' && (
          <HttpConfiguration node={node} workflow={workflow} document={document} path={`${nodePath}/config`} />
        )}
        {mode === 'edit' &&
          document &&
          workflow &&
          node.type === 'call' &&
          !authenticationDefinition(document, node) && (
            <CallConfiguration node={node} workflow={workflow} document={document} path={`${nodePath}/config`} />
          )}
        {mode === 'edit' && workflow && node.type === 'extract' && (
          <ExtractEditor key={`${workflow.id}:${node.id}`} node={node} workflow={workflow} />
        )}
        {mode === 'edit' && workflow && node.type === 'setSession' && (
          <SessionEditor key={`${workflow.id}:${node.id}`} node={node} workflow={workflow} />
        )}
        {mode === 'edit' && (
          <section className="position-editor">
            <h3>Canvas position</h3>

            <form
              onSubmit={handleSubmit((values) => {
                const rounded = roundPosition(values);
                move(node.id, rounded);
                reset(rounded);
              })}
            >
              <div className="coordinate-fields">
                <label>
                  X
                  <input
                    type="number"
                    step="any"
                    {...register('x', { valueAsNumber: true })}
                    aria-invalid={Boolean(errors.x)}
                  />
                </label>
                <label>
                  Y
                  <input
                    type="number"
                    step="any"
                    {...register('y', { valueAsNumber: true })}
                    aria-invalid={Boolean(errors.y)}
                  />
                </label>
              </div>

              {(errors.x || errors.y) && (
                <p role="alert" className="field-error">
                  Enter finite numeric coordinates.
                </p>
              )}
              <Button variant="outline" size="sm" type="submit">
                Apply position
              </Button>
            </form>
          </section>
        )}
      </div>
    </section>
  );
}

function HttpEditor({
  node,
  workflowId,
}: {
  node: Extract<import('../../lib/scenario-types').WorkflowNode, { type: 'http' }>;
  workflowId: string;
}) {
  const document = useScenarioStore((state) => state.document)!;
  const workflowIndex = document.workflows.findIndex((workflow) => workflow.id === workflowId);
  const nodeIndex = document.workflows[workflowIndex]?.nodes.findIndex((value) => value.id === node.id);
  const fieldPath = `/workflows/${workflowIndex}/nodes/${nodeIndex}`;
  const [path, setPath] = useState(node.config.path);
  const key = `http:${workflowId}:${node.id}:path`;
  useEffect(() => {
    setPath(node.config.path);
  }, [node.config.path]);
  useEffect(() => {
    useScenarioStore.getState().setFormError(key, path.trim() ? null : 'Enter a request path.');
    return () => useScenarioStore.getState().setFormError(key, null);
  }, [key, path]);
  return (
    <section className="http-editor">
      <h3>HTTP Request</h3>
      <NodeBaseUrl workflowId={workflowId} node={node} />
      <label>
        Method
        <select
          data-field-path={`${fieldPath}/config/method`}
          value={node.config.method}
          onChange={(e) =>
            useScenarioStore.getState().command({
              type: 'node.put',
              workflowId,
              node: { ...node, config: { ...node.config, method: e.target.value as typeof node.config.method } },
            })
          }
        >
          {['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].map((method) => (
            <option key={method}>{method}</option>
          ))}
        </select>
      </label>
      <label>
        Path
        <input
          data-field-path={`${fieldPath}/config/path`}
          value={path}
          placeholder="/health"
          aria-invalid={!path.trim()}
          onChange={(e) => {
            const value = e.target.value;
            setPath(value);
            if (value.trim())
              useScenarioStore
                .getState()
                .command(
                  { type: 'node.put', workflowId, node: { ...node, config: { ...node.config, path: value } } },
                  `path:${node.id}`
                );
          }}
          onBlur={() => useScenarioStore.getState().endHistoryGroup()}
        />
      </label>
      {!path.trim() && (
        <p className="field-error" role="alert">
          Enter a request path, for example /health.
        </p>
      )}
    </section>
  );
}

export function NodeInspector() {
  const view = useRunView();
  return view.viewing ? <RunInspector /> : <ScenarioInspector />;
}
function ScenarioInspector() {
  const state = useScenarioStore();
  const document = state.document;
  const workflow = document ? selectedWorkflow(document, state.selectedWorkflowId) : undefined;
  const workflowPath = `/workflows/${document?.workflows.indexOf(workflow!)}`;
  useEffect(() => {
    if (!state.focusedFieldPath) return;
    const frame = window.setTimeout(() => {
      const field = Array.from(window.document.querySelectorAll<HTMLElement>('[data-field-path]'))
        .filter(
          (element) =>
            element.dataset.fieldPath === state.focusedFieldPath ||
            state.focusedFieldPath!.startsWith(`${element.dataset.fieldPath}/`)
        )
        .sort((a, b) => (b.dataset.fieldPath?.length ?? 0) - (a.dataset.fieldPath?.length ?? 0))[0];
      const control = field?.matches('input, select, textarea')
        ? field
        : field?.querySelector<HTMLElement>('input, select, textarea, button');
      control?.focus();
      control?.scrollIntoView?.({ block: 'nearest' });
    });
    return () => window.clearTimeout(frame);
  }, [state.focusedFieldPath, state.inspectorTab, state.selectedId, state.mode]);
  return (
    <aside className="inspector" aria-label="Node inspector">
      <h2>Inspector</h2>
      <Tabs
        className="inspector-tabs"
        value={state.inspectorTab}
        onValueChange={(tab) => state.setInspectorTab(tab as typeof state.inspectorTab)}
      >
        <TabsList aria-label="Inspector sections">
          <TabsTrigger
            value="node"
            disabled={state.inspectorTab !== 'node' && Object.keys(state.formErrors).length > 0}
          >
            Node
          </TabsTrigger>
          <TabsTrigger
            value="workflow"
            disabled={state.inspectorTab !== 'workflow' && Object.keys(state.formErrors).length > 0}
          >
            Workflow
          </TabsTrigger>
          <TabsTrigger
            value="scenario"
            disabled={state.inspectorTab !== 'scenario' && Object.keys(state.formErrors).length > 0}
          >
            Scenario
          </TabsTrigger>
        </TabsList>
        <TabsContent value="node">
          <NodeContent />
        </TabsContent>
        <TabsContent value="workflow">
          <div className="inspector-content">
            <FieldDiagnostics prefix={workflowPath} />
            {document &&
            state.canvasView === 'lifecycle' &&
            (!workflow || !ownsPhase(document, state.selectedPhase)) ? (
              <section>
                <h3>{phaseName[state.selectedPhase]}</h3>
                {workflow ? (
                  <>
                    <Button size="sm" variant="ghost" onClick={() => state.selectWorkflow(workflow.id)}>
                      Open workflow
                    </Button>
                    {state.mode === 'edit' && (
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => state.command({ type: 'phase.detach', phase: state.selectedPhase })}
                      >
                        Detach copy
                      </Button>
                    )}
                  </>
                ) : state.mode === 'edit' ? (
                  <AddNodePopover phase={state.selectedPhase} />
                ) : (
                  <p className="muted">Not configured</p>
                )}
              </section>
            ) : (
              document &&
              workflow &&
              (state.mode === 'edit' ? (
                <WorkflowConfiguration key={workflow.id} document={document} workflow={workflow} path={workflowPath} />
              ) : (
                <>
                  <h3>Workflow ports</h3>
                  <p className="muted">Inputs: {Object.keys(workflow.inputs).join(', ') || 'None'}</p>
                  <p className="muted">Outputs: {Object.keys(workflow.outputs).join(', ') || 'None'}</p>
                </>
              ))
            )}
          </div>
        </TabsContent>
        <TabsContent value="scenario">
          <div className="inspector-content">
            <FieldDiagnostics prefix="scenario" />
            {document &&
              (state.mode === 'edit' ? (
                <ScenarioConfiguration document={document} />
              ) : (
                <>
                  <ScenarioOverview document={document} />
                </>
              ))}
          </div>
        </TabsContent>
      </Tabs>
    </aside>
  );
}
