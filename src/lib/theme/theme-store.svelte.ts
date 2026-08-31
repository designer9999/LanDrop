import { generateColorTokens, type SchemeVariant, type M3ColorTokens } from "./m3-color";

const STORAGE_KEY = "landrop-theme";

interface ThemeConfig {
  seedColor: string;
  variant: SchemeVariant;
  isDark: boolean;
  mica: boolean;
  micaOpacity: number;
}

function loadConfig(): ThemeConfig {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const cfg = JSON.parse(raw);
      // Never allow mica in light mode
      if (cfg.mica && !cfg.isDark) cfg.mica = false;
      return cfg;
    }
  } catch {}
  return {
    seedColor: "#6750A4",
    variant: "expressive",
    isDark: true,
    mica: false,
    micaOpacity: 70,
  };
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
  mica = $state(cached.mica);
  micaOpacity = $state(cached.micaOpacity ?? 70);
  tokens: M3ColorTokens = $derived(generateColorTokens(this.seedColor, this.variant, this.isDark));

  persist() {
    saveConfig({
      seedColor: this.seedColor,
      variant: this.variant,
      isDark: this.isDark,
      mica: this.mica,
      micaOpacity: this.micaOpacity,
    });
  }
}

let instance: ThemeState | null = null;

export function getThemeState(): ThemeState {
  if (!instance) instance = new ThemeState();
  return instance;
}
