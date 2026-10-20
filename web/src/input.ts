// DOM input into the Grid: a persistent transparent textarea at the focused
// cell holds DOM focus and takes keys through the keymap; pointer events go
// to the Grid's raw pointer input with the widget's own click count. Typing,
// composing and the edit keys go to the editor (see `editing.ts`).
//
// Touch pans the native scroller (`touch-action: pan-x pan-y`), so a touch on
// the body becomes a press only once it ends as a tap. The header bands are
// covered by elements with `touch-action: none`, where a finger drags like a
// mouse: resizing and moving headers work by touch too.

import { createClickCounter } from "./clicks";
import { AXIS_CODE, FOCUS_STEP, POINTER, modifierBits } from "./codes";
import { createEditing, type EditHost, type Editing } from "./editing";
import type { Effect } from "./effects";
import type { Action, Keymap } from "./keymap";
import type { Cursor, Rect, Snapshot } from "./snapshot";
import type { WebGrid } from "./wasm";

export interface InputHost {
  grid: WebGrid;
  /** The element covering the Grid's viewport: pointer coordinates are relative to it. */
  layer: HTMLElement;
  keymap: Keymap;
  /** After each DOM input: drain Effects and schedule a frame. */
  changed(): void;
  /** The widget gained or lost DOM focus. */
  focusChanged(): void;
  /** The last frame's Snapshot. */
  snapshot(): Snapshot | null;
  /** What the editor needs from the widget. */
  edits: Pick<EditHost, "editable" | "value" | "text" | "editor" | "commit" | "theme">;
  /** Whether (x, y) is on a column menu button. */
  menuButtonAt(x: number, y: number): boolean;
  /** A press at (x, y), before the Grid sees it: `true` if a column menu button took it. */
  menuPress(x: number, y: number): boolean;
  /** Opens the focused column's menu: `false` if there is none to open. */
  openColumnMenu(): boolean;
  /** Opens the find bar, or steps while find is open: `false` if there is no find bar or find is closed. */
  find(action: "open" | "next" | "previous"): boolean;
}

export interface Input {
  /** The textarea that holds DOM focus. */
  readonly element: HTMLTextAreaElement;
  /** Whether the widget has DOM focus. */
  readonly focused: boolean;
  /** Answers the edit Effects; `false` for the rest. */
  answer(effect: Effect): boolean;
  /** Follows a frame: moves the textarea to the focused cell (or the editor to the edited one), sets the pointer shape. */
  place(s: Snapshot): void;
  destroy(): void;
}

/** How far a header border reaches into the items beside it, CSS px, for a mouse and for a finger. */
const GRAB_FINE = 3;
const GRAB_COARSE = 12;

const TEXTAREA_STYLE = [
  "position: absolute",
  "margin: 0",
  "padding: 0",
  "border: 0",
  "outline: none",
  "resize: none",
  "overflow: hidden",
  "white-space: pre",
  "background: transparent",
  "color: transparent",
  "caret-color: transparent",
  "pointer-events: none",
  "z-index: 1",
].join("; ");

const BAND_STYLE = "position: absolute; left: 0; top: 0; touch-action: none;";

const CURSORS: Record<Cursor, string> = {
  default: "",
  resizeCol: "col-resize",
  resizeRow: "row-resize",
  move: "grabbing",
};

export function perform(grid: WebGrid, action: Action): void {
  switch (action.type) {
    case "MoveFocus":
      return grid.move_focus(AXIS_CODE[action.axis], FOCUS_STEP[action.by], action.forward, action.extend);
    case "SelectAll":
      return grid.select_all();
    case "SelectColumn":
      return grid.select_column();
    case "SelectRow":
      return grid.select_row();
    case "Cancel":
      return grid.cancel();
    case "Activate":
      return grid.activate();
    case "BeginEdit":
      return grid.begin_edit();
    case "ScrollBy":
      return grid.scroll_by(AXIS_CODE[action.axis], action.items);
    case "FindNext":
      return grid.find_next();
    case "FindPrevious":
      return grid.find_previous();
    case "OpenColumnMenu":
    case "OpenFind":
      // The widget's, not the Grid's: see `createInput`.
      return;
  }
}

interface Touch {
  id: number;
  x: number;
  y: number;
  moved: boolean;
}

export function createInput(host: InputHost): Input {
  const { grid, layer } = host;
  const textarea = document.createElement("textarea");
  textarea.className = "binnacle-input";
  textarea.style.cssText = TEXTAREA_STYLE;
  textarea.setAttribute("autocomplete", "off");
  textarea.setAttribute("autocorrect", "off");
  textarea.setAttribute("autocapitalize", "off");
  textarea.spellcheck = false;
  textarea.rows = 1;
  const bands = { top: band("binnacle-band-top"), left: band("binnacle-band-left") };
  layer.append(textarea, bands.top, bands.left);
  const editing: Editing = createEditing(
    {
      ...host.edits,
      grid,
      layer,
      textarea,
      refocus: () => takeFocus(),
      changed: () => host.changed(),
    },
    TEXTAREA_STYLE,
  );

  const clicks = createClickCounter();
  const cleanup: (() => void)[] = [];
  let focused = false;
  // A press is moving DOM focus: the Grid's focus comes from the press, not from tabbing in.
  let pressing = false;
  // The pointer whose press the Grid is following, until its release.
  let held: number | null = null;
  // A touch on the body, which becomes a press only if it ends as a tap.
  let touch: Touch | null = null;
  let placed = { x: NaN, y: NaN, width: NaN, height: NaN };
  let cursor = "";
  let overButton = false;

  const on = <K extends keyof HTMLElementEventMap>(
    target: HTMLElement,
    type: K,
    handler: (e: HTMLElementEventMap[K]) => void,
  ) => {
    target.addEventListener(type, handler);
    cleanup.push(() => target.removeEventListener(type, handler));
  };

  const at = (e: { clientX: number; clientY: number }) => {
    const box = layer.getBoundingClientRect();
    return { x: e.clientX - box.left, y: e.clientY - box.top };
  };

  const takeFocus = () => {
    pressing = true;
    textarea.focus({ preventScroll: true });
    pressing = false;
  };

  const send = (kind: number, e: PointerEvent, x: number, y: number, count = 1) =>
    grid.pointer(kind, x, y, e.button < 0 ? 0 : e.button, modifierBits(e), count);

  const press = (e: PointerEvent, x: number, y: number) => {
    clearSelection();
    send(POINTER.down, e, x, y, clicks.press({ time: e.timeStamp, x, y, button: e.button }));
  };

  on(textarea, "keydown", (e) => {
    if (e.isComposing) return;
    if (editing.open) {
      if (editing.key(e)) host.changed();
      return;
    }
    const action = host.keymap.lookup(e);
    if (!action) return;
    if (action.type === "OpenColumnMenu") {
      if (host.openColumnMenu()) e.preventDefault();
      return;
    }
    if (action.type === "OpenFind" || action.type === "FindNext" || action.type === "FindPrevious") {
      const asked = action.type === "OpenFind" ? "open" : action.type === "FindNext" ? "next" : "previous";
      if (host.find(asked)) e.preventDefault();
      return;
    }
    e.preventDefault();
    perform(grid, action);
    host.changed();
  });
  // Typing starts an edit with the typed text; read-only, the text goes nowhere.
  on(textarea, "input", (e) => {
    if (editing.open || (e as InputEvent).isComposing) return;
    if (textarea.value) editing.type();
    if (!editing.open) textarea.value = "";
  });
  // Composing starts an edit at once, so the candidate window opens at the cell.
  on(textarea, "compositionstart", () => editing.type());
  on(textarea, "compositionend", () => {
    if (!editing.open) textarea.value = "";
  });
  on(textarea, "focus", () => {
    if (!pressing) focusFirstCell();
    host.changed();
  });
  // DOM focus may move between the textarea and a custom editor: the widget
  // has it while it is anywhere inside.
  on(layer, "focusin", () => {
    if (focused) return;
    focused = true;
    host.focusChanged();
  });
  on(layer, "focusout", (e) => {
    const to = e.relatedTarget;
    if (to instanceof Node && layer.contains(to)) return;
    // Focus leaving the page (another window) keeps the edit open; leaving the widget commits it.
    if (editing.open && document.hasFocus()) {
      editing.commit();
      host.changed();
    }
    focused = false;
    host.focusChanged();
  });

  on(layer, "pointerdown", (e) => {
    if (e.target === textarea || editing.owns(e.target)) return;
    // A press elsewhere commits the edit before the Grid sees it.
    editing.commit();
    const { x, y } = at(e);
    const finger = e.pointerType === "touch" && e.target !== bands.top && e.target !== bands.left;
    if (e.button === 0 && host.menuPress(x, y)) {
      e.preventDefault();
      return;
    }
    if (finger) {
      // Panning is the browser's; a tap is decided on release.
      touch = { id: e.pointerId, x, y, moved: false };
      return;
    }
    e.preventDefault();
    press(e, x, y);
    if (e.button === 0) {
      held = e.pointerId;
      layer.setPointerCapture(e.pointerId);
    }
    takeFocus();
    host.changed();
  });
  on(layer, "pointermove", (e) => {
    if (editing.owns(e.target)) return;
    const { x, y } = at(e);
    if (touch?.id === e.pointerId) {
      touch.moved ||= Math.hypot(x - touch.x, y - touch.y) > 8;
      return;
    }
    if (e.pointerType === "touch" && held !== e.pointerId) return;
    overButton = held === null && host.menuButtonAt(x, y);
    send(POINTER.move, e, x, y);
    host.changed();
  });
  on(layer, "pointerup", (e) => {
    if (editing.owns(e.target)) return;
    const { x, y } = at(e);
    if (touch?.id === e.pointerId) {
      const tap = !touch.moved;
      touch = null;
      if (!tap) return;
      press(e, x, y);
      send(POINTER.up, e, x, y);
      takeFocus();
      host.changed();
      return;
    }
    if (held === e.pointerId) held = null;
    send(POINTER.up, e, x, y);
    host.changed();
  });
  // A mouse press, real or made up after a tap, would move DOM focus off the textarea.
  on(layer, "mousedown", (e) => {
    if (!editing.owns(e.target)) e.preventDefault();
  });
  // The browser took the pointer away (a pan, a lost capture): end the gesture as if cancelled.
  const lost = (e: PointerEvent) => {
    if (touch?.id === e.pointerId) touch = null;
    if (held !== e.pointerId) return;
    held = null;
    grid.cancel();
    host.changed();
  };
  on(layer, "pointercancel", lost);
  on(layer, "lostpointercapture", lost);
  on(layer, "pointerleave", (e) => {
    if (held !== null || e.pointerType === "touch") return;
    // A point outside the viewport ends hover.
    grid.pointer(POINTER.move, NaN, NaN, 0, 0, 0);
    host.changed();
  });

  const coarse = matchMedia("(pointer: coarse)");
  const grab = () => grid.set_border_grab(coarse.matches ? GRAB_COARSE : GRAB_FINE);
  grab();
  coarse.addEventListener("change", grab);
  cleanup.push(() => coarse.removeEventListener("change", grab));

  /**
   * Preventing a press's default keeps DOM focus, but also stops the browser
   * collapsing the page's selection, so a press does it here. In WebKit the
   * focused textarea's caret is that selection: it gets its caret back.
   */
  function clearSelection() {
    const selection = getSelection();
    if (!selection || selection.isCollapsed) return;
    selection.removeAllRanges();
    if (document.activeElement === textarea) textarea.setSelectionRange(textarea.value.length, textarea.value.length);
  }

  /** Tabbing in with no cell focused focuses the first visible one. */
  function focusFirstCell() {
    const s = host.snapshot();
    if (!s || s.rows.focus || s.cols.focus) return;
    const first = (items: Snapshot["rows"]) =>
      items.strip.find((i) => i.key !== null && i.offset + i.size > items.header && i.offset < items.extent)?.key;
    const row = first(s.rows);
    const col = first(s.cols);
    if (row != null && col != null) grid.set_focus(row, col);
  }

  return {
    element: textarea,
    get focused() {
      return focused;
    },
    answer: (effect) => editing.answer(effect),
    place(s) {
      if (editing.open) {
        editing.place(s);
        placed = { x: NaN, y: NaN, width: NaN, height: NaN };
      }
      const r = within(s.focusRect, s) ?? { x: s.cols.header, y: s.rows.header, width: 1, height: 1 };
      if (!editing.open && (r.x !== placed.x || r.y !== placed.y || r.width !== placed.width || r.height !== placed.height)) {
        placed = r;
        Object.assign(textarea.style, {
          left: `${r.x}px`,
          top: `${r.y}px`,
          width: `${r.width}px`,
          height: `${r.height}px`,
        });
      }
      const shape = overButton && s.cursor === "default" ? "pointer" : CURSORS[s.cursor];
      if (shape !== cursor) layer.style.cursor = cursor = shape;
      size(bands.top, s.cols.extent, s.rows.header);
      size(bands.left, s.cols.header, s.rows.extent);
    },
    destroy() {
      for (const undo of cleanup) undo();
      editing.destroy();
      textarea.remove();
      bands.top.remove();
      bands.left.remove();
    },
  };
}

/** The part of `r` inside the body, past both header bands; `null` if none of it is. */
function within(r: Rect | null, s: Snapshot): Rect | null {
  if (!r) return null;
  const x0 = Math.max(r.x, s.cols.header);
  const y0 = Math.max(r.y, s.rows.header);
  const x1 = Math.min(r.x + r.width, s.cols.extent);
  const y1 = Math.min(r.y + r.height, s.rows.extent);
  return x1 > x0 && y1 > y0 ? { x: x0, y: y0, width: x1 - x0, height: y1 - y0 } : null;
}

function band(className: string): HTMLDivElement {
  const el = document.createElement("div");
  el.className = className;
  el.style.cssText = BAND_STYLE;
  return el;
}

function size(el: HTMLElement, width: number, height: number): void {
  const w = `${width}px`;
  const h = `${height}px`;
  if (el.style.width !== w) el.style.width = w;
  if (el.style.height !== h) el.style.height = h;
}
