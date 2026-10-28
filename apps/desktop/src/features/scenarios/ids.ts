export function allocateId(prefix: string, used: string[]): string {
  let n = 1;
  while (used.includes(`${prefix}-${n}`)) n++;
  return `${prefix}-${n}`;
}
