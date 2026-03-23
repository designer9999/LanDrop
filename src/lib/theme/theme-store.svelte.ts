import { generateColorTokens, type SchemeVariant, type M3ColorTokens } from "./m3-color";

const STORAGE_KEY = "landrop-theme";

interface ThemeConfig {
  seedColor: string;
  variant: SchemeVariant;
  isDark: boolean;
}

function loadConfig(): ThemeConfig {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return JSON.parse(raw);
  } catch {}
  return { seedColor: "#6750A4", variant: "expressive", isDark: true };
}

function saveConfig(cfg: ThemeConfig) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(cfg));
  } catch {}
}

const cached = loadConfig();

class ThemeState {
  seedColor = $state(cached.seedColor);
  variant = $state<SchemeVariant>(cached.variant);
  isDark = $state(cached.isDark);
  tokens: M3ColorTokens = $derived(generateColorTokens(this.seedColor, this.variant, this.isDark));

  persist() {
    saveConfig({ seedColor: this.seedColor, variant: this.variant, isDark: this.isDark });
  }
}

let instance: ThemeState | null = null;

export function getThemeState(): ThemeState {
  if (!instance) instance = new ThemeState();
  return instance;
}
