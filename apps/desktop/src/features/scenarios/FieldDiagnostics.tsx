import { useScenarioStore } from './store';
export function FieldDiagnostics({ prefix }: { prefix: string }) {
  const diagnostics = useScenarioStore((state) => state.diagnostics);
  const inspect = useScenarioStore((state) => state.inspectDiagnostic);
  const matching = diagnostics.filter((diagnostic) =>
    prefix === 'scenario'
      ? !diagnostic.fieldPath.startsWith('/workflows/')
      : diagnostic.fieldPath === prefix || diagnostic.fieldPath.startsWith(`${prefix}/`)
  );
  return matching.length ? (
    <section className="field-diagnostics" aria-label="Configuration diagnostics">
      {matching.map((diagnostic, index) => (
        <button key={index} onClick={() => inspect(diagnostic)}>
          <strong>{diagnostic.message}</strong>
          <small>
            {diagnostic.fieldPath}
            {diagnostic.expressionLine
              ? ` · expression ${diagnostic.expressionLine}:${diagnostic.expressionColumn}`
              : ''}
          </small>
        </button>
      ))}
    </section>
  ) : null;
}
