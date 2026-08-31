import type { M3ColorTokens } from "./m3-color";
import { tokensToCssVars } from "./m3-tokens";

export function applyThemeToDOM(tokens: M3ColorTokens): void {
  const vars = tokensToCssVars(tokens);
  const root = document.documentElement;
  for (const [prop, value] of Object.entries(vars)) {
    root.style.setProperty(prop, value);
  }
  // Cache the surface pair for the pre-paint boot script (public/boot-theme.js)
  // so the next launch's first frame matches this theme.
  try {
    localStorage.setItem(
      "landrop-theme-boot",
      JSON.stringify({ surface: tokens.surface, onSurface: tokens.onSurface }),
    );
  } catch {
    /* non-fatal */
  }
}
