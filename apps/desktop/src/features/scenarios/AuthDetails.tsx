import { extractionOutputs } from './extraction';
import type { z } from 'zod';
import type { ScenarioDocument, WorkflowNode, templateValueSchema } from '../../lib/scenario-types';

type TemplateValue = z.infer<typeof templateValueSchema>;
// Inspect persisted references only. Literal payload/header values are intentionally hidden.
function valueReferences(value: TemplateValue, path: string): string[] {
  switch (value.source) {
    case 'identity':
      return [`${path}: identity.${value.field}`];
    case 'extraction':
      return [`${path}: ${value.nodeId}.${value.name}`];
    case 'environment':
      return [`${path}: environment.${value.name}`];
    case 'input':
      return [`${path}: input.${value.name}`];
    case 'callOutput':
      return [`${path}: ${value.nodeId}.${value.name}`];
    case 'response':
      return [`${path}: ${value.nodeId}.${value.field}`];
    case 'literal':
      return [`${path}: literal value (hidden)`];
    case 'object':
      return Object.entries(value.fields).flatMap(([key, item]) => valueReferences(item, `${path}.${key}`));
    case 'array':
      return value.items.flatMap((item, index) => valueReferences(item, `${path}[${index}]`));
  }
}

export function AuthDetails({ node, document }: { node: WorkflowNode; document: ScenarioDocument }) {
  const groups = document.groups.filter((group) => group.identityAssignment.identityId !== null);
  return (
    <>
      {node.type === 'http' && node.config.body?.kind === 'jsonTemplate' && (
        <section className="auth-details" aria-label="Request value references">
          <h3>JSON template</h3>
          <ul>
            {Object.entries(node.config.body.fields)
              .flatMap(([key, value]) => valueReferences(value, key))
              .map((reference, index) => (
                <li key={index}>
                  <code>{reference}</code>
                </li>
              ))}
          </ul>
        </section>
      )}
      {node.type === 'extract' && (
        <section className="auth-details" aria-label="Extraction configuration">
          <h3>Extraction</h3>
          <dl className="details">
            <div>
              <dt>Source node</dt>
              <dd>
                <code>{node.config.sourceNodeId}</code>
              </dd>
            </div>
            {Object.entries(extractionOutputs(node.config)).map(([name, output]) => (
              <div key={name}>
                <dt>
                  {name} · {output.type}
                </dt>
                <dd>
                  <code>{output.expression}</code>
                </dd>
              </div>
            ))}
          </dl>
        </section>
      )}
      {node.type === 'setSession' && (
        <section className="auth-details" aria-label="Session header operations">
          <h3>Session headers</h3>
          <dl className="details">
            {Object.entries(node.config.setHeaders).map(([name, source]) => (
              <div key={name}>
                <dt>Set</dt>
                <dd>
                  <code>{name}</code>
                  <br />
                  <span>
                    {source.source === 'identity'
                      ? `identity.${source.field}`
                      : source.source === 'extraction'
                        ? `${source.nodeId}.${source.name}`
                        : source.source === 'input'
                          ? `input.${source.name}`
                          : source.source === 'environment'
                            ? `environment.${source.name}`
                            : source.source === 'callOutput'
                              ? `${source.nodeId}.${source.name}`
                              : 'Literal value (hidden)'}
                  </span>
                  {source.prefix && <span> · Prefix configured (hidden)</span>}
                </dd>
              </div>
            ))}
            {node.config.removeHeaders.map((name) => (
              <div key={`remove-${name}`}>
                <dt>Remove</dt>
                <dd>
                  <code>{name}</code>
                </dd>
              </div>
            ))}
          </dl>
        </section>
      )}
      {groups.length > 0 && (
        <section className="auth-details" aria-label="Assigned identity references">
          <h3>Assigned identities</h3>
          {groups.map((group) => {
            const identity = document.identities.find((item) => item.id === group.identityAssignment.identityId);
            return (
              <div key={group.id}>
                <p className="muted">
                  {group.id}: <code>{group.identityAssignment.identityId}</code>
                </p>
                <dl className="details">
                  {identity &&
                    Object.entries(identity.credentials).map(([field, ref]) => (
                      <div key={field}>
                        <dt>{field}</dt>
                        <dd>
                          Environment
                          <br />
                          <code>{ref.name}</code>
                        </dd>
                      </div>
                    ))}
                </dl>
              </div>
            );
          })}
        </section>
      )}
    </>
  );
}
