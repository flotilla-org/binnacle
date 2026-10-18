// Themes: the colours, fonts and line widths the canvas painter draws in, by
// role. The roles and presets follow the ratatui Themes; the painter decides
// how each role looks on a canvas.

export interface Style {
  color: string;
  background: string;
}

/** A line the painter draws on top of the cells, `width` in CSS px. */
export interface Line {
  color: string;
  width: number;
}

export interface Theme {
  name: string;
  /** Body text and row numbers; headers use `headerFont`. */
  font: string;
  headerFont: string;
  /** Column headers. */
  header: Style;
  /** The header (or row number) under the pointer. */
  headerHovered: Style;
  /** Row numbers, down the left band. */
  rowNumber: Style;
  body: Style;
  /** Every other row's background, by Position; `null` leaves rows unstriped. */
  zebra: string | null;
  /** The ring around the focused cell, drawn while the widget has DOM focus. */
  focus: Line;
  /** Laid over the selected cells and the headers of selected rows and columns: keep `background` translucent. */
  selection: Style;
  /** A picked row: `color` marks it beside its number. */
  picks: Style;
  sortMarker: string;
  /** The lines a resize grabs, in the header bands' Gaps. */
  handle: string;
  /** The line a resize in progress draws across the grid. */
  guide: Line;
  dropIndicator: Line;
  /** The Border Style's lines in the body, and the band rules. */
  border: string;
  /** The marker where a Key or value is not there yet, and the message in an empty table's body. */
  loading: string;
  /** The marker on a cell whose request the Table Source failed. */
  error: string;
  /** A cell whose value just changed. */
  flash: Style;
  /** A cell that matches the Find Query: `background` fills it under its value. */
  findMatch: Style;
  /** The current match: `background` fills it and `color` rings it. */
  findCurrent: Style;
  /** Laid over the cells of an unmatched row (kept after an edit though the filter drops it), dimming it: keep it translucent. */
  unmatched: string;
  /**
   * Fills for pending states: a band of `shimmerHighlight` sweeps over
   * `shimmerBase` across the header of a sort not answered yet, and the
   * focused cell pulses in `shimmerBase` while its Locate is pending.
   */
  shimmerBase: string;
  shimmerHighlight: string;
}

const SANS = 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif';

export const light: Theme = {
  name: "light",
  font: `13px ${SANS}`,
  headerFont: `600 12px ${SANS}`,
  header: { color: "#334155", background: "#f1f5f9" },
  headerHovered: { color: "#0f172a", background: "#e2e8f0" },
  rowNumber: { color: "#64748b", background: "#f8fafc" },
  body: { color: "#0f172a", background: "#ffffff" },
  zebra: "#fafbfc",
  focus: { color: "#2563eb", width: 2 },
  selection: { color: "#0f172a", background: "rgb(37 99 235 / 0.12)" },
  picks: { color: "#2563eb", background: "#eff6ff" },
  sortMarker: "#2563eb",
  handle: "#b8c2cf",
  guide: { color: "#2563eb", width: 2 },
  dropIndicator: { color: "#0891b2", width: 2 },
  border: "#d8dee6",
  loading: "#94a3b8",
  error: "#dc2626",
  flash: { color: "#0f172a", background: "#fef08a" },
  findMatch: { color: "#0f172a", background: "#fde68a" },
  findCurrent: { color: "#ea580c", background: "#fdba74" },
  unmatched: "rgb(255 255 255 / 0.6)",
  shimmerBase: "#dbeafe",
  shimmerHighlight: "#ffffff",
};

/** Catppuccin's Mocha flavour, with zebra stripes. */
const mocha = {
  crust: "#11111b",
  mantle: "#181825",
  base: "#1e1e2e",
  surface0: "#313244",
  surface1: "#45475a",
  surface2: "#585b70",
  overlay0: "#6c7086",
  text: "#cdd6f4",
  mauve: "#cba6f7",
  peach: "#fab387",
  red: "#f38ba8",
  yellow: "#f9e2af",
  sapphire: "#74c7ec",
  blue: "#89b4fa",
};

export const catppuccinMocha: Theme = {
  name: "catppuccin mocha",
  font: light.font,
  headerFont: light.headerFont,
  header: { color: mocha.blue, background: mocha.crust },
  headerHovered: { color: mocha.sapphire, background: mocha.surface0 },
  rowNumber: { color: mocha.overlay0, background: mocha.crust },
  body: { color: mocha.text, background: mocha.base },
  zebra: mocha.mantle,
  focus: { color: mocha.blue, width: 2 },
  selection: { color: mocha.text, background: "rgb(137 180 250 / 0.18)" },
  picks: { color: mocha.mauve, background: mocha.surface0 },
  sortMarker: mocha.peach,
  handle: mocha.surface2,
  guide: { color: mocha.yellow, width: 2 },
  dropIndicator: { color: mocha.sapphire, width: 2 },
  border: mocha.surface0,
  loading: mocha.overlay0,
  error: mocha.red,
  flash: { color: mocha.crust, background: mocha.yellow },
  findMatch: { color: mocha.text, background: "rgb(249 226 175 / 0.22)" },
  findCurrent: { color: mocha.peach, background: "rgb(250 179 135 / 0.45)" },
  unmatched: "rgb(30 30 46 / 0.6)",
  shimmerBase: mocha.surface0,
  shimmerHighlight: mocha.surface2,
};

/** Greys only, for printing and for pages that want no colour. */
export const mono: Theme = {
  name: "mono",
  font: light.font,
  headerFont: light.headerFont,
  header: { color: "#000000", background: "#ebebeb" },
  headerHovered: { color: "#000000", background: "#d6d6d6" },
  rowNumber: { color: "#555555", background: "#f5f5f5" },
  body: { color: "#000000", background: "#ffffff" },
  zebra: null,
  focus: { color: "#000000", width: 2 },
  selection: { color: "#000000", background: "rgb(0 0 0 / 0.12)" },
  picks: { color: "#000000", background: "#ececec" },
  sortMarker: "#000000",
  handle: "#8c8c8c",
  guide: { color: "#000000", width: 1 },
  dropIndicator: { color: "#000000", width: 2 },
  border: "#bfbfbf",
  loading: "#8c8c8c",
  error: "#000000",
  flash: { color: "#ffffff", background: "#000000" },
  findMatch: { color: "#000000", background: "#d9d9d9" },
  findCurrent: { color: "#000000", background: "#a6a6a6" },
  unmatched: "rgb(255 255 255 / 0.6)",
  shimmerBase: "#d4d4d4",
  shimmerHighlight: "#f5f5f5",
};

/** Every preset, in the order a host cycles them. */
export const presets: readonly Theme[] = [light, catppuccinMocha, mono];

export type ColorScheme = "light" | "dark";

/**
 * What the widget draws in: a Theme, or one per colour scheme, asked again
 * whenever `prefers-color-scheme` flips.
 */
export type ThemeChoice = Theme | ((scheme: ColorScheme) => Theme);

/** The default: light, or Catppuccin Mocha in a dark colour scheme. */
export const systemTheme = (scheme: ColorScheme): Theme => (scheme === "dark" ? catppuccinMocha : light);

/**
 * A Theme from `--binnacle-*` custom properties on `element` (inherited ones
 * count), over `base`. Each role is a property named after it in kebab case:
 * `--binnacle-border`, `--binnacle-header-font`, and for roles with parts,
 * one per part: `--binnacle-body-color`, `--binnacle-focus-width` (CSS px).
 * `--binnacle-zebra: none` turns stripes off. Pass it as a `ThemeChoice` to
 * re-read the properties when the colour scheme flips.
 */
export function themeFromCss(element: Element, base: Theme = light): Theme {
  const style = getComputedStyle(element);
  const read = (path: string): string | null => style.getPropertyValue(`--binnacle-${kebab(path)}`).trim() || null;
  const value = <T>(path: string, was: T): T => {
    const raw = read(path);
    if (raw === null) return was;
    if (typeof was === "number") {
      const n = Number.parseFloat(raw);
      return (Number.isFinite(n) ? n : was) as T;
    }
    if (path === "zebra" && raw === "none") return null as T;
    return raw as T;
  };
  const theme: Record<string, unknown> = { ...base, name: "css" };
  for (const [role, was] of Object.entries(base)) {
    if (role === "name") continue;
    if (was !== null && typeof was === "object") {
      const parts: Record<string, unknown> = {};
      for (const [part, v] of Object.entries(was)) parts[part] = value(`${role}-${part}`, v);
      theme[role] = parts;
    } else {
      theme[role] = value(role, was);
    }
  }
  return theme as unknown as Theme;
}

function kebab(name: string): string {
  return name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
}
