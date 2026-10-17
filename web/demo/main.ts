// The demo page: a toolbar to pick a source, the grid, and a status line.

import { GridWidget, type FrameInfo } from "../src";
import { SOURCES, type DemoSource } from "./sources";

const host = document.querySelector<HTMLElement>("#grid")!;
const tabs = document.querySelector<HTMLElement>("#sources")!;
const status = document.querySelector<HTMLElement>("#status")!;

const number = new Intl.NumberFormat("en");
let current: GridWidget | null = null;
let shown: DemoSource | null = null;

function describe(frame: FrameInfo): string {
  const { rows, cols } = frame;
  return [
    shown?.name ?? "",
    `rows ${number.format(rows.first + 1)} to ${number.format(rows.last + 1)} of ${number.format(rows.count)}`,
    `columns ${number.format(cols.first + 1)} to ${number.format(cols.last + 1)} of ${number.format(cols.count)}`,
  ].join("  ·  ");
}

async function show(source: DemoSource) {
  current?.destroy();
  shown = source;
  for (const tab of tabs.querySelectorAll("button")) tab.setAttribute("aria-pressed", String(tab.dataset.source === source.name));
  const url = new URL(location.href);
  url.searchParams.set("source", source.name);
  history.replaceState(null, "", url);
  current = await GridWidget.create(host, {
    ...source.options,
    onFrame: (frame) => {
      status.textContent = describe(frame);
    },
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

const wanted = new URL(location.href).searchParams.get("source");
void show(SOURCES.find((s) => s.name === wanted) ?? SOURCES[0]!);
