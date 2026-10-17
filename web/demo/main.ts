// The demo page: a toolbar to pick a source, a Theme and a Border Style, the
// grid, and a status line. The choices live in the URL, so a reload keeps them.

import {
  borderName,
  borderStyles,
  catppuccinMocha,
  presets,
  systemTheme,
  type BorderStyle,
  type FrameInfo,
  type Theme,
  type ThemeChoice,
} from "../src";
import { SOURCES, type DemoSource, type Mounted } from "./sources";

const host = document.querySelector<HTMLElement>("#grid")!;
const tabs = document.querySelector<HTMLElement>("#sources")!;
const status = document.querySelector<HTMLElement>("#status")!;
const themes = document.querySelector<HTMLSelectElement>("#theme")!;
const follow = document.querySelector<HTMLInputElement>("#follow")!;
const borders = document.querySelector<HTMLSelectElement>("#border")!;
const toolbar = document.querySelector<HTMLElement>("#tools")!;

const number = new Intl.NumberFormat("en");
let current: Mounted | null = null;
let shown: DemoSource | null = null;
const params = new URL(location.href).searchParams;
let pickedTheme: Theme = presets.find((t) => t.name === params.get("theme")) ?? presets[0]!;
let following = params.get("theme") === null;
let border: BorderStyle = borderStyles.find((b) => borderName(b) === params.get("border")) ?? borderStyles[2]!;

const themeChoice = (): ThemeChoice => (following ? systemTheme : pickedTheme);

function describe(frame: FrameInfo): string {
  const { rows, cols, focus, selection, picked } = frame;
  const theme = current?.theme;
  const parts = [
    shown?.name ?? "",
    `theme ${theme?.name ?? ""}${following ? " (system)" : ""}`,
    `border ${borderName(border)}`,
    `rows ${number.format(rows.first + 1)} to ${number.format(rows.last + 1)} of ${number.format(rows.count)}`,
    `columns ${number.format(cols.first + 1)} to ${number.format(cols.last + 1)} of ${number.format(cols.count)}`,
  ];
  if (focus) {
    const ids = shown?.showIds && frame.focusIds ? ` (${String(frame.focusIds.row)} / ${String(frame.focusIds.col)})` : "";
    parts.push(`focus row ${number.format(focus.row + 1)}, column ${number.format(focus.col + 1)}${ids}`);
  }
  if (selection) {
    const size = (s: { start: number; end: number }) => number.format(s.end - s.start);
    parts.push(`selected ${size(selection.rows)} × ${size(selection.cols)}`);
  }
  if (picked) parts.push(`${number.format(picked)} picked`);
  if (frame.loading) parts.push("loading");
  if (frame.editing) parts.push("editing");
  return parts.join("  ·  ");
}

/** Mirrors the grid's Theme in the toolbar and the page's own colours. */
function reflect(frame: FrameInfo): void {
  const theme = current?.theme;
  if (theme) {
    themes.value = theme.name;
    document.documentElement.dataset.scheme = theme === catppuccinMocha ? "dark" : "light";
  }
  status.textContent = describe(frame);
}

function remember(): void {
  const url = new URL(location.href);
  url.searchParams.set("source", shown?.name ?? "");
  if (following) url.searchParams.delete("theme");
  else url.searchParams.set("theme", pickedTheme.name);
  url.searchParams.set("border", borderName(border));
  history.replaceState(null, "", url);
}

async function show(source: DemoSource) {
  current?.destroy();
  current = null;
  shown = source;
  for (const tab of tabs.querySelectorAll("button")) tab.setAttribute("aria-pressed", String(tab.dataset.source === source.name));
  remember();
  current = await source.mount(host, toolbar, {
    theme: themeChoice(),
    border,
    onFrame: reflect,
  });
}

for (const source of SOURCES) {
  const tab = document.createElement("button");
  tab.type = "button";
  tab.dataset.source = source.name;
  tab.textContent = source.name;
  tab.title = source.description;
  tab.addEventListener("click", () => void show(source));
  tabs.append(tab);
}

for (const theme of presets) themes.add(new Option(theme.name, theme.name));
themes.value = pickedTheme.name;
themes.disabled = following;
follow.checked = following;
themes.addEventListener("change", () => {
  pickedTheme = presets.find((t) => t.name === themes.value) ?? pickedTheme;
  if (current) current.theme = themeChoice();
  remember();
});
follow.addEventListener("change", () => {
  following = follow.checked;
  themes.disabled = following;
  // Unfollowing keeps the Theme on screen until another is picked.
  if (!following && current) pickedTheme = current.theme;
  if (current) current.theme = themeChoice();
  remember();
});

for (const style of borderStyles) borders.add(new Option(borderName(style), borderName(style)));
borders.value = borderName(border);
borders.addEventListener("change", () => {
  border = borderStyles.find((b) => borderName(b) === borders.value) ?? border;
  if (current) current.border = border;
  remember();
});

void show(SOURCES.find((s) => s.name === params.get("source")) ?? SOURCES[0]!);
