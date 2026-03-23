/**
 * M3 Dynamic Color — HCT algorithm wrapper
 *
 * Uses @material/material-color-utilities to generate all 26+ color roles
 * from a single seed color + scheme variant + dark/light mode.
 */
import {
  argbFromHex,
  hexFromArgb,
  Hct,
  SchemeTonalSpot,
  SchemeVibrant,
  SchemeExpressive,
  SchemeFidelity,
  SchemeContent,
  SchemeMonochrome,
  SchemeNeutral,
  SchemeRainbow,
  SchemeFruitSalad,
  type DynamicScheme,
} from "@material/material-color-utilities";

export type SchemeVariant =
  | "tonalSpot"
  | "vibrant"
  | "expressive"
  | "fidelity"
  | "content"
  | "monochrome"
  | "neutral"
  | "rainbow"
  | "fruitSalad";

const SCHEME_CONSTRUCTORS: Record<
  SchemeVariant,
  new (hct: Hct, isDark: boolean, contrast: number) => DynamicScheme
> = {
  tonalSpot: SchemeTonalSpot,
  vibrant: SchemeVibrant,
  expressive: SchemeExpressive,
  fidelity: SchemeFidelity,
  content: SchemeContent,
  monochrome: SchemeMonochrome,
  neutral: SchemeNeutral,
  rainbow: SchemeRainbow,
  fruitSalad: SchemeFruitSalad,
};

export const VARIANT_INFO: {
  id: SchemeVariant;
  name: string;
  desc: string;
}[] = [
  { id: "tonalSpot", name: "Tonal Spot", desc: "Balanced, safe default" },
  { id: "vibrant", name: "Vibrant", desc: "Bold and saturated" },
  { id: "expressive", name: "Expressive", desc: "Creative hue shifts" },
  { id: "fidelity", name: "Fidelity", desc: "Preserves seed color" },
  { id: "content", name: "Content", desc: "Seed in containers" },
  { id: "neutral", name: "Neutral", desc: "Whisper of color" },
  { id: "monochrome", name: "Monochrome", desc: "Pure grayscale" },
  { id: "rainbow", name: "Rainbow", desc: "Colorful surfaces" },
  { id: "fruitSalad", name: "Fruit Salad", desc: "Playful shifted hues" },
];

export const PRESET_COLORS = [
  "#6750A4",
  "#B91C1C",
  "#0D9488",
  "#2563EB",
  "#D97706",
  "#7C3AED",
  "#059669",
  "#DC2626",
  "#4F46E5",
  "#EA580C",
  "#0891B2",
  "#C026D3",
  "#65A30D",
  "#E11D48",
  "#0284C7",
  "#FACC15",
];

export interface M3ColorTokens {
  primary: string;
  onPrimary: string;
  primaryContainer: string;
  onPrimaryContainer: string;
  secondary: string;
  onSecondary: string;
  secondaryContainer: string;
  onSecondaryContainer: string;
  tertiary: string;
  onTertiary: string;
  tertiaryContainer: string;
  onTertiaryContainer: string;
  error: string;
  onError: string;
  errorContainer: string;
  onErrorContainer: string;
  surface: string;
  onSurface: string;
  onSurfaceVariant: string;
  surfaceDim: string;
  surfaceBright: string;
  surfaceContainerLowest: string;
  surfaceContainerLow: string;
  surfaceContainer: string;
  surfaceContainerHigh: string;
  surfaceContainerHighest: string;
  outline: string;
  outlineVariant: string;
  inverseSurface: string;
  inverseOnSurface: string;
  inversePrimary: string;
  shadow: string;
  scrim: string;
}

/**
 * Generate all M3 color tokens from a seed color.
 */
export function generateColorTokens(
  seedHex: string,
  variant: SchemeVariant = "expressive",
  isDark: boolean = true,
  contrastLevel: number = 0.0
): M3ColorTokens {
  const seedArgb = argbFromHex(seedHex);
  const seedHct = Hct.fromInt(seedArgb);
  const SchemeClass = SCHEME_CONSTRUCTORS[variant];
  const scheme = new SchemeClass(seedHct, isDark, contrastLevel);

  return {
    primary: hexFromArgb(scheme.primary),
    onPrimary: hexFromArgb(scheme.onPrimary),
    primaryContainer: hexFromArgb(scheme.primaryContainer),
    onPrimaryContainer: hexFromArgb(scheme.onPrimaryContainer),
    secondary: hexFromArgb(scheme.secondary),
    onSecondary: hexFromArgb(scheme.onSecondary),
    secondaryContainer: hexFromArgb(scheme.secondaryContainer),
    onSecondaryContainer: hexFromArgb(scheme.onSecondaryContainer),
    tertiary: hexFromArgb(scheme.tertiary),
    onTertiary: hexFromArgb(scheme.onTertiary),
    tertiaryContainer: hexFromArgb(scheme.tertiaryContainer),
    onTertiaryContainer: hexFromArgb(scheme.onTertiaryContainer),
    error: hexFromArgb(scheme.error),
    onError: hexFromArgb(scheme.onError),
    errorContainer: hexFromArgb(scheme.errorContainer),
    onErrorContainer: hexFromArgb(scheme.onErrorContainer),
    surface: hexFromArgb(scheme.surface),
    onSurface: hexFromArgb(scheme.onSurface),
    onSurfaceVariant: hexFromArgb(scheme.onSurfaceVariant),
    surfaceDim: hexFromArgb(scheme.surfaceDim),
    surfaceBright: hexFromArgb(scheme.surfaceBright),
    surfaceContainerLowest: hexFromArgb(scheme.surfaceContainerLowest),
    surfaceContainerLow: hexFromArgb(scheme.surfaceContainerLow),
    surfaceContainer: hexFromArgb(scheme.surfaceContainer),
    surfaceContainerHigh: hexFromArgb(scheme.surfaceContainerHigh),
    surfaceContainerHighest: hexFromArgb(scheme.surfaceContainerHighest),
    outline: hexFromArgb(scheme.outline),
    outlineVariant: hexFromArgb(scheme.outlineVariant),
    inverseSurface: hexFromArgb(scheme.inverseSurface),
    inverseOnSurface: hexFromArgb(scheme.inverseOnSurface),
    inversePrimary: hexFromArgb(scheme.inversePrimary),
    shadow: hexFromArgb(scheme.shadow),
    scrim: hexFromArgb(scheme.scrim),
  };
}
