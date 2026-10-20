// The keymap: key chords to Actions. The defaults follow platform
// conventions (Cmd on macOS, Ctrl elsewhere); the widget user extends or
// overrides them. A key with no Action is left to the page.

import type { AxisName, PinEdge } from "./codes";

export type FocusStep = "item" | "page" | "edge";

/** A semantic input the widget sends the Grid on the user's behalf. */
export type Action =
  | { type: "MoveFocus"; axis: AxisName; by: FocusStep; forward: boolean; extend: boolean }
  | { type: "SelectAll" }
  | { type: "SelectColumn" }
  | { type: "SelectRow" }
  | { type: "Cancel" }
  | { type: "Activate" }
  | { type: "BeginEdit" }
  | { type: "ScrollBy"; axis: AxisName; items: number }
  /** Opens the focused column's menu, if a column menu is mounted; the key is the page's otherwise. */
  | { type: "OpenColumnMenu" }
  /** Opens the focused row's menu, to filter the columns, if a row menu is mounted; the key is the page's otherwise. */
  | { type: "OpenRowMenu" }
  /** Opens the find bar, if one is mounted and the Table Source finds; the key is the page's otherwise. */
  | { type: "OpenFind" }
  /** Steps to the next or previous match while find is open; the key is the page's otherwise. */
  | { type: "FindNext" }
  | { type: "FindPrevious" }
  /** Pins the focused item of `axis` to `edge`, or unpins it if it is pinned there already. */
  | { type: "Pin"; axis: AxisName; edge: PinEdge }
  /** Unpins the focused item of `axis`. */
  | { type: "Unpin"; axis: AxisName }
  /** Hides the focused item of `axis`, if the Table Source hides that axis; the key is the page's otherwise. */
  | { type: "Hide"; axis: AxisName }
  /** Opens the focused item's menu at the list of hidden items of `axis`, if a menu is mounted and the axis hides; the key is the page's otherwise. */
  | { type: "ShowHidden"; axis: AxisName }
  /** Pins every item of `axis` from the start up to the focused one, as far as fits. */
  | { type: "Freeze"; axis: AxisName };

export type Platform = "mac" | "other";

/**
 * Chords to Actions. A chord is modifiers then a key, joined by `+`:
 * `"Mod+Shift+ArrowDown"`, `"F2"`, `"Ctrl+Space"`. Modifiers are Ctrl, Alt,
 * Shift, Meta (or Cmd) and Mod, which is Cmd on macOS and Ctrl elsewhere; the
 * key is `KeyboardEvent.key`, with `Space`, `Plus` and `Esc` accepted. Case
 * does not matter. `null` unbinds a default, leaving the key to the page.
 */
export type KeyBindings = Record<string, Action | null>;

/** What a lookup reads from a `KeyboardEvent`. */
export interface KeyChord {
  key: string;
  /**
   * The physical key, which a lookup falls back on for a letter or digit
   * when `key` names no binding: macOS gives Option+0 the key `º`.
   */
  code?: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

export interface Keymap {
  /** The Action for a key event, or `undefined` if the key is not the grid's. */
  lookup(event: KeyChord): Action | undefined;
}

export interface KeymapOptions {
  /** Merged over the defaults; a later chord naming the same keys wins. */
  bindings?: KeyBindings;
  /** Defaults to the platform the page runs on. */
  platform?: Platform;
  /** Start from the default bindings (the default) or from none. */
  defaults?: boolean;
}

export function detectPlatform(): Platform {
  const nav = globalThis.navigator as (Navigator & { userAgentData?: { platform?: string } }) | undefined;
  const name = nav?.userAgentData?.platform ?? nav?.platform ?? "";
  return /mac|iphone|ipad|ipod/i.test(name) ? "mac" : "other";
}

const MODIFIER_NAMES: Record<string, "ctrl" | "alt" | "shift" | "meta" | "mod"> = {
  ctrl: "ctrl",
  control: "ctrl",
  alt: "alt",
  option: "alt",
  shift: "shift",
  meta: "meta",
  cmd: "meta",
  command: "meta",
  mod: "mod",
};

const KEY_NAMES: Record<string, string> = { space: " ", plus: "+", esc: "escape" };

/** The canonical form both chords and events reduce to. */
function canonical(key: string, ctrl: boolean, alt: boolean, shift: boolean, meta: boolean): string {
  return `${ctrl ? "C" : "-"}${alt ? "A" : "-"}${shift ? "S" : "-"}${meta ? "M" : "-"}${key.toLowerCase()}`;
}

function parse(chord: string, platform: Platform): string {
  // A trailing "++" or a lone "+" names the plus key.
  const plus = chord === "+" || chord.endsWith("++");
  const parts = (plus ? chord.slice(0, -1) : chord).split("+");
  const name = plus ? "+" : parts.pop()!;
  const held = new Set<string>();
  for (const part of plus ? parts.slice(0, -1) : parts) {
    let m = MODIFIER_NAMES[part.toLowerCase()];
    if (!m) throw new Error(`binnacle: unknown modifier "${part}" in key chord "${chord}"`);
    if (m === "mod") m = platform === "mac" ? "meta" : "ctrl";
    held.add(m);
  }
  if (name === "") throw new Error(`binnacle: key chord "${chord}" names no key`);
  const key = KEY_NAMES[name.toLowerCase()] ?? name;
  return canonical(key, held.has("ctrl"), held.has("alt"), held.has("shift"), held.has("meta"));
}

const move = (axis: AxisName, by: FocusStep, forward: boolean, extend: boolean): Action => ({
  type: "MoveFocus",
  axis,
  by,
  forward,
  extend,
});

/**
 * The default bindings: arrows, Shift to extend, Mod+arrow to the edge,
 * pages (Alt+PageUp and Alt+PageDown along the columns), Home and End (the
 * first and last row), Mod+A, Ctrl+Space the column and Shift+Space the row,
 * Alt+ArrowDown (or Shift+F10, or the context menu key) for the column menu
 * and Alt+ArrowRight for the row menu, Mod+F for the find bar, F3 (Shift+F3
 * back) to step between matches, Alt+Shift with an arrow to pin the focused
 * column left or right, or its row to the top or bottom (or unpin it),
 * Alt+Shift+Backspace to unpin the column and Mod+Alt+Shift+Backspace the
 * row, Alt+Shift+Home to freeze the columns up to the focused one and
 * Mod+Alt+Shift+Home the rows, Mod+Alt+0 to hide the focused column and
 * Mod+Alt+9 its row (as Google Sheets has them), with Shift to list the
 * hidden ones. Pinning avoids Alt with a letter, which types a symbol on
 * macOS.
 */
export function defaultBindings(): KeyBindings {
  const bindings: KeyBindings = {};
  const moves: [string, AxisName, FocusStep, boolean][] = [
    ["ArrowDown", "rows", "item", true],
    ["ArrowUp", "rows", "item", false],
    ["ArrowRight", "cols", "item", true],
    ["ArrowLeft", "cols", "item", false],
    ["PageDown", "rows", "page", true],
    ["PageUp", "rows", "page", false],
    ["Alt+PageDown", "cols", "page", true],
    ["Alt+PageUp", "cols", "page", false],
    ["End", "rows", "edge", true],
    ["Home", "rows", "edge", false],
    ["Mod+ArrowDown", "rows", "edge", true],
    ["Mod+ArrowUp", "rows", "edge", false],
    ["Mod+ArrowRight", "cols", "edge", true],
    ["Mod+ArrowLeft", "cols", "edge", false],
  ];
  for (const [chord, axis, by, forward] of moves) {
    bindings[chord] = move(axis, by, forward, false);
    bindings[`Shift+${chord}`] = move(axis, by, forward, true);
  }
  bindings["Mod+A"] = { type: "SelectAll" };
  bindings["Ctrl+Space"] = { type: "SelectColumn" };
  bindings["Shift+Space"] = { type: "SelectRow" };
  bindings["Escape"] = { type: "Cancel" };
  bindings["Enter"] = { type: "Activate" };
  bindings["F2"] = { type: "BeginEdit" };
  bindings["Alt+ArrowDown"] = { type: "OpenColumnMenu" };
  bindings["Alt+ArrowRight"] = { type: "OpenRowMenu" };
  bindings["Mod+F"] = { type: "OpenFind" };
  bindings["F3"] = { type: "FindNext" };
  bindings["Shift+F3"] = { type: "FindPrevious" };
  bindings["Alt+Shift+ArrowLeft"] = { type: "Pin", axis: "cols", edge: "leading" };
  bindings["Alt+Shift+ArrowRight"] = { type: "Pin", axis: "cols", edge: "trailing" };
  bindings["Alt+Shift+ArrowUp"] = { type: "Pin", axis: "rows", edge: "leading" };
  bindings["Alt+Shift+ArrowDown"] = { type: "Pin", axis: "rows", edge: "trailing" };
  bindings["Alt+Shift+Backspace"] = { type: "Unpin", axis: "cols" };
  bindings["Shift+F10"] = { type: "OpenColumnMenu" };
  bindings["ContextMenu"] = { type: "OpenColumnMenu" };
  bindings["Mod+Alt+0"] = { type: "Hide", axis: "cols" };
  bindings["Mod+Alt+9"] = { type: "Hide", axis: "rows" };
  bindings["Mod+Alt+Shift+0"] = { type: "ShowHidden", axis: "cols" };
  bindings["Mod+Alt+Shift+9"] = { type: "ShowHidden", axis: "rows" };
  bindings["Mod+Alt+Shift+Backspace"] = { type: "Unpin", axis: "rows" };
  bindings["Alt+Shift+Home"] = { type: "Freeze", axis: "cols" };
  bindings["Mod+Alt+Shift+Home"] = { type: "Freeze", axis: "rows" };
  return bindings;
}

export function createKeymap(options: KeymapOptions = {}): Keymap {
  const platform = options.platform ?? detectPlatform();
  const table = new Map<string, Action | null>();
  const bind = (bindings: KeyBindings) => {
    for (const [chord, action] of Object.entries(bindings)) table.set(parse(chord, platform), action);
  };
  if (options.defaults ?? true) bind(defaultBindings());
  if (options.bindings) bind(options.bindings);
  const physical = (code: string | undefined) => /^(?:Key([A-Z])|Digit(\d))$/.exec(code ?? "")?.slice(1).find(Boolean);
  return {
    lookup: (e) => {
      const chord = (key: string) => canonical(key, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
      const named = chord(e.key);
      if (table.has(named)) return table.get(named) ?? undefined;
      const key = physical(e.code);
      return key === undefined ? undefined : (table.get(chord(key)) ?? undefined);
    },
  };
}
