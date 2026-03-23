import { generateColorTokens, type M3ColorTokens } from "./m3-color";

const SEED_COLOR = "#6750A4";

class ThemeState {
  seedColor = $state(SEED_COLOR);
  isDark = $state(true);
  tokens: M3ColorTokens = $derived(generateColorTokens(this.seedColor, this.isDark));
}

let instance: ThemeState | null = null;

export function getThemeState(): ThemeState {
  if (!instance) instance = new ThemeState();
  return instance;
}
