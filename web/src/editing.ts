// In-place editing. The input's textarea is the default editor: it already
// holds DOM focus at the focused cell, so typing or composing there starts an
// edit with the text kept, and F2 (or Enter, or a double-click) starts one with
// the cell's text. A custom editor mounts in an overlay over the cell instead.
// Either way the Grid hears `edit_started` within the event that asked, and
// `edit_ended` when the editor closes.

import { PAD } from "./canvas/context";
import { AXIS_CODE, EDIT_END, FOCUS_STEP } from "./codes";
import type { Effect } from "./effects";
import type { Rect, Snapshot } from "./snapshot";
import type { Theme } from "./theme";
import type { WebGrid } from "./wasm";

/** A custom editor for a cell, from the widget's `editor` option. */
export interface Editor<V = unknown> {
  /**
   * Builds the editor inside `el`, which covers the cell. `initial` is the
   * cell's value (`undefined` while loading); `typed` is the text typed to
   * start the edit, or `null` if it began from F2, Enter or a double-click.
   */
  mount(el: HTMLElement, initial: V | undefined, typed: string | null): void;
  /** The value to commit. */
  value(): V;
  /** Takes DOM focus, right after mounting. */
  focus(): void;
  /** Removes what `mount` built. */
  destroy(): void;
}

export interface EditHost {
  grid: WebGrid;
  layer: HTMLElement;
  textarea: HTMLTextAreaElement;
  /** Whether edits may start at all: the widget was given `onCommit`. */
  editable: boolean;
  /** The value shown for the cell with these Keys. */
  value(row: bigint, col: bigint): unknown;
  /** The cell's text, as its Cell Renderer gives it: what the textarea starts with. */
  text(row: bigint, col: bigint): string;
  /** The custom editor for the cell, or `null` for the textarea. */
  editor(row: bigint, col: bigint): Editor | null;
  /** Hands a committed value to the widget user. */
  commit(row: bigint, col: bigint, value: unknown): void;
  theme(): Theme;
  /** Gives DOM focus back to the textarea. */
  refocus(): void;
  /** After a key the editor handled: drain Effects and schedule a frame. */
  changed(): void;
}

export interface Editing {
  /** Whether an editor is open. */
  readonly open: boolean;
  /** Whether `node` is part of the open editor, so input there is the editor's own. */
  owns(node: EventTarget | null): boolean;
  /** Answers the edit Effects (`EditRequested`, `Activated`, `EditCancelled`); `false` for the rest. */
  answer(effect: Effect): boolean;
  /** Starts an edit of the focused cell from what was typed or is being composed in the textarea. */
  type(): void;
  /** Handles a key while editing: Enter commits, Tab and Shift+Tab commit and move, Escape cancels. */
  key(e: KeyboardEvent): boolean;
  /** Closes the editor, keeping its value. */
  commit(): void;
  /** Follows a frame: keeps the editor over its cell, clipped to the body. */
  place(s: Snapshot): void;
  destroy(): void;
}

const OVERLAY_STYLE = "position: absolute; z-index: 2; box-sizing: border-box; display: none;";

interface Open {
  row: bigint;
  col: bigint;
  custom: Editor | null;
}

export function createEditing(host: EditHost, idleStyle: string): Editing {
  const { grid, textarea } = host;
  const overlay = document.createElement("div");
  overlay.className = "binnacle-editor";
  overlay.style.cssText = OVERLAY_STYLE;
  host.layer.append(overlay);
  let open: Open | null = null;
  // The Theme the textarea was last dressed in, so a Theme change mid-edit restyles it.
  let dressed: Theme | null = null;
  // Set while an edit started by typing is being asked for, so it keeps the typed text.
  let typing = false;

  const onOverlayKey = (e: KeyboardEvent) => {
    if (editing.key(e)) host.changed();
  };
  overlay.addEventListener("keydown", onOverlayKey);

  const start = (row: bigint, col: bigint) => {
    const value = host.value(row, col);
    const custom = host.editor(row, col);
    const typed = typing ? textarea.value : null;
    open = { row, col, custom };
    if (custom) {
      textarea.value = "";
      overlay.style.display = "block";
      over(overlay, rectOf(textarea));
      custom.mount(overlay, value, typed);
      custom.focus();
    } else {
      if (typed === null) {
        textarea.value = host.text(row, col);
        textarea.setSelectionRange(textarea.value.length, textarea.value.length);
      }
      dress(textarea, (dressed = host.theme()));
    }
    grid.edit_started(row, col);
  };

  /** Puts the textarea back to its idle state and drops a custom editor. */
  const close = () => {
    const custom = open?.custom;
    open = null;
    textarea.value = "";
    textarea.style.cssText = idleStyle + "; " + placement(textarea);
    if (custom) {
      const had = overlay.contains(document.activeElement);
      custom.destroy();
      overlay.replaceChildren();
      overlay.style.display = "none";
      if (had) host.refocus();
    }
  };

  const editing: Editing = {
    get open() {
      return open !== null;
    },
    owns: (node) => open !== null && node instanceof Node && (node === textarea || overlay.contains(node)),
    answer(effect) {
      switch (effect.type) {
        case "EditRequested":
        case "Activated":
          if (host.editable && !open) start(effect.row, effect.col);
          return true;
        case "EditCancelled":
          if (open) close();
          return true;
        default:
          return false;
      }
    },
    type() {
      if (!host.editable || open) return;
      typing = true;
      grid.begin_edit();
      host.changed();
      typing = false;
    },
    key(e) {
      if (!open || e.isComposing) return false;
      if (e.key === "Enter" && !e.altKey) {
        editing.commit();
      } else if (e.key === "Tab") {
        editing.commit();
        grid.move_focus(AXIS_CODE.cols, FOCUS_STEP.item, !e.shiftKey, false);
      } else if (e.key === "Escape") {
        close();
        grid.edit_ended(EDIT_END.cancel);
      } else {
        return false;
      }
      e.preventDefault();
      e.stopPropagation();
      return true;
    },
    commit() {
      if (!open) return;
      const { row, col, custom } = open;
      const value = custom ? custom.value() : textarea.value;
      close();
      grid.edit_ended(EDIT_END.commit);
      host.commit(row, col, value);
    },
    place(s) {
      if (!open) return;
      const r = s.editorRect;
      const el = open.custom ? overlay : textarea;
      if (!open.custom && dressed !== host.theme()) dress(textarea, (dressed = host.theme()));
      if (!r) {
        el.style.clipPath = "inset(50%)";
        return;
      }
      over(el, r);
      const top = Math.max(0, s.rows.header - r.y);
      const left = Math.max(0, s.cols.header - r.x);
      const bottom = Math.max(0, r.y + r.height - s.rows.extent);
      const right = Math.max(0, r.x + r.width - s.cols.extent);
      el.style.clipPath = top || left || bottom || right ? `inset(${top}px ${right}px ${bottom}px ${left}px)` : "";
    },
    destroy() {
      if (open) close();
      overlay.removeEventListener("keydown", onOverlayKey);
      overlay.remove();
    },
  };
  return editing;
}

/** Makes the idle, see-through textarea look like a cell being edited. */
function dress(textarea: HTMLTextAreaElement, theme: Theme): void {
  const height = textarea.style.height;
  Object.assign(textarea.style, {
    boxSizing: "border-box",
    padding: `0 ${PAD}px`,
    lineHeight: height,
    font: theme.font,
    color: theme.body.color,
    background: theme.body.background,
    caretColor: theme.body.color,
    outline: `${theme.focus.width}px solid ${theme.focus.color}`,
    outlineOffset: `${-theme.focus.width}px`,
    pointerEvents: "auto",
  });
}

function over(el: HTMLElement, r: Rect): void {
  Object.assign(el.style, { left: `${r.x}px`, top: `${r.y}px`, width: `${r.width}px`, height: `${r.height}px` });
  if (el instanceof HTMLTextAreaElement) el.style.lineHeight = `${r.height}px`;
}

function rectOf(el: HTMLElement): Rect {
  const px = (v: string) => parseFloat(v) || 0;
  return { x: px(el.style.left), y: px(el.style.top), width: px(el.style.width), height: px(el.style.height) };
}

function placement(el: HTMLElement): string {
  const { left, top, width, height } = el.style;
  return `left: ${left}; top: ${top}; width: ${width}; height: ${height}`;
}
