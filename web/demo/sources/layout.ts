// Saving and restoring a demo grid's layout: Save keeps `widget.layout()` in
// the browser's storage per demo, Restore puts it back, and a saved layout is
// restored as the grid mounts, before its data arrives, as an app would.

import type { GridWidget, Layout } from "../../src";

const storageKey = (demo: string) => `binnacle-demo-layout:${demo}`;

function load<R, C>(demo: string): Layout<R, C> | null {
  try {
    const text = localStorage.getItem(storageKey(demo));
    return text ? (JSON.parse(text) as Layout<R, C>) : null;
  } catch {
    return null;
  }
}

/** Save and Restore buttons for `widget`'s layout, and its saved layout put back now. */
export function layoutControls<R, C>(widget: GridWidget<R, C>, demo: string): { elements: HTMLElement[]; destroy(): void } {
  const state = document.createElement("output");
  state.className = "layout-state";
  const button = (text: string, title: string, act: () => void) => {
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = text;
    b.title = title;
    b.addEventListener("click", act);
    return b;
  };
  const save = button("Save layout", "Keep the sort, filter, hidden and pinned items, sizes and header bands", () => {
    const layout = widget.layout();
    try {
      localStorage.setItem(storageKey(demo), JSON.stringify(layout));
      state.textContent = "saved";
    } catch {
      state.textContent = "can't save here";
    }
  });
  const restore = button("Restore layout", "Put back the saved layout", () => {
    const layout = load<R, C>(demo);
    state.textContent = !layout ? "nothing saved" : widget.restoreLayout(layout) ? "restored" : "saved layout too new";
  });
  const saved = load<R, C>(demo);
  if (saved && widget.restoreLayout(saved)) state.textContent = "restored";
  return {
    elements: [save, restore, state],
    destroy() {
      save.remove();
      restore.remove();
      state.remove();
    },
  };
}
