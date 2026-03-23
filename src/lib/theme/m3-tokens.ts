import type { M3ColorTokens } from "./m3-color";

function toKebab(key: string): string {
  return key.replace(/([A-Z])/g, "-$1").toLowerCase();
}

export function tokensToCssVars(tokens: M3ColorTokens): Record<string, string> {
  const vars: Record<string, string> = {};
  for (const [key, value] of Object.entries(tokens)) {
    vars[`--md-sys-color-${toKebab(key)}`] = value;
  }
  return vars;
}
