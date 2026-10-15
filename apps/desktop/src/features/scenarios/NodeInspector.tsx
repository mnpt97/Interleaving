import { iterationWorkflow } from '../../lib/scenario-types';
import { AuthDetails } from './AuthDetails';
import { nodePresentation } from './node-presentation';
import { useEffect } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { z } from 'zod';
import { Button } from '../../components/ui/button';
import { useScenarioStore } from './store';

const positionSchema = z.object({ x: z.number().finite(), y: z.number().finite() });
type Coordinates = z.infer<typeof positionSchema>;

export function NodeInspector() {
  const document = useScenarioStore((s) => s.document);
  const selectedId = useScenarioStore((s) => s.selectedId);
  const move = useScenarioStore((s) => s.move);
  const workflow = document ? iterationWorkflow(document) : undefined;
  const index = workflow?.nodes.findIndex((node) => node.id === selectedId) ?? -1;
  const node = workflow?.nodes[index];
  const position = selectedId ? workflow?.layout?.positions[selectedId] : undefined;
  const x = position?.x ?? 80 + index * 260;
  const y = position?.y ?? 160;
  const {
    register,
    handleSubmit,
    reset,
    formState: { errors },
  } = useForm<Coordinates>({ resolver: zodResolver(positionSchema), defaultValues: { x, y } });
  useEffect(() => {
    reset({ x, y });
  }, [selectedId, x, y, reset]);
  if (!node)
    return (
      <aside className="inspector">
        <h2>Inspector</h2>
        <p className="muted inspector-empty">Select a node to inspect its configuration and position.</p>
      </aside>
    );
  return (
    <aside className="inspector" aria-label="Node inspector">
      <h2>Inspector</h2>
      <div className="inspector-content">
        <h3>{node.id}</h3>
        <p className="muted">{nodePresentation[node.type].description}</p>
        <dl className="details">
          <div>
            <dt>Node ID</dt>
            <dd>
              <code>{node.id}</code>
            </dd>
          </div>
          <div>
            <dt>Type</dt>
            <dd>{node.type}</dd>
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
        {document && <AuthDetails node={node} document={document} />}
        <section className="position-editor">
          <h3>Canvas position</h3>
          <p className="muted">Positions do not change execution order.</p>
          <form onSubmit={handleSubmit((values) => move(node.id, values))}>
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
        <p className="inspector-note">Workflow configuration is read-only. Use your scenario file to change it.</p>
      </div>
    </aside>
  );
}
