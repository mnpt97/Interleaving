import { diagnosticSchema } from '../scenario-types';
export function errorMessage(
  error: unknown,
  fallback = 'The desktop operation failed. Try again and check file permissions.'
) {
  const diagnostics = diagnosticSchema.array().nonempty().max(100).safeParse(error);
  if (diagnostics.success) return diagnostics.data.map((item) => `${item.code}: ${item.message}`).join(' ');
  const diagnostic = diagnosticSchema.safeParse(error);
  return diagnostic.success ? `${diagnostic.data.code}: ${diagnostic.data.message}` : fallback;
}
