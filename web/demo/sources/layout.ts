// Saving and restoring a demo grid's layout: Save keeps `widget.layout()` in
// the browser's storage per demo, Restore puts it back, and a saved layout is
// restored as the grid mounts, before its data arrives, as an app would. The
// buttons sit at the end of the page's second toolbar line (`#layout`).

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

/** Shows Save and Restore buttons for `widget`'s layout, and puts its saved layout back now. */
export function layoutControls<R, C>(widget: GridWidget<R, C>, demo: string): { destroy(): void } {
  const state = document.createElement("output");
  state.className = "layout-state";
  const caption = document.createElement("span");
  caption.textContent = "Layout";
  const button = (text: string, title: string, act: () => void) => {
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = text;
    b.setAttribute("aria-label", `${text} layout`);
    b.title = title;
    b.addEventListener("click", act);
    return b;
  };
  const save = button("Save", "Keep the sort, filter, hidden and pinned items, sizes and header bands", () => {
    const layout = widget.layout();
    try {
      localStorage.setItem(storageKey(demo), JSON.stringify(layout));
      state.textContent = "saved";
    } catch {
      state.textContent = "not saved";
    }
  });
  const restore = button("Restore", "Put back the saved layout", () => {
    const layout = load<R, C>(demo);
    state.textContent = !layout ? "none saved" : widget.restoreLayout(layout) ? "restored" : "too new";
  });
  const saved = load<R, C>(demo);
  if (saved && widget.restoreLayout(saved)) state.textContent = "restored";
  document.querySelector("#layout")?.append(caption, save, restore, state);
  return {
    destroy() {
      caption.remove();
      save.remove();
      restore.remove();
      state.remove();
    },
  };
}
