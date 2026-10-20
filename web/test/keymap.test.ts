// Seam 1: the keymap, as a widget user sees it. Chords are written the way a
// person would ("Mod+Shift+ArrowDown") and looked up from key events.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { expect, test } from "vitest";
import { createKeymap, type Action, type KeyChord, type Platform } from "../src/keymap";

type Modifier = "ctrl" | "alt" | "shift" | "meta";

const MODIFIERS: readonly Modifier[] = ["ctrl", "alt", "shift", "meta"];

/** The ways a person may spell each modifier in a chord. */
const SPELLINGS: Record<Modifier, readonly string[]> = {
  ctrl: ["Ctrl", "Control", "ctrl", "CTRL"],
  alt: ["Alt", "Option", "alt"],
  shift: ["Shift", "shift"],
  meta: ["Meta", "Cmd", "Command", "meta"],
};

/** Keys as `KeyboardEvent.key` reports them, beside how a chord may name them. */
const KEYS: readonly { event: string; names: readonly string[] }[] = [
  { event: "ArrowDown", names: ["ArrowDown", "arrowdown"] },
  { event: "PageUp", names: ["PageUp", "pageup"] },
  { event: "Home", names: ["Home"] },
  { event: "Enter", names: ["Enter", "enter"] },
  { event: "Escape", names: ["Escape", "Esc"] },
  { event: "F2", names: ["F2", "f2"] },
  { event: " ", names: ["Space", "space"] },
  { event: "+", names: ["+", "Plus"] },
  { event: "a", names: ["a", "A"] },
  { event: "7", names: ["7"] },
];

const ACTIONS: readonly Action[] = [
  { type: "SelectAll" },
  { type: "SelectColumn" },
  { type: "SelectRow" },
  { type: "Cancel" },
  { type: "Activate" },
  { type: "BeginEdit" },
  { type: "OpenColumnMenu" },
  { type: "OpenRowMenu" },
  { type: "MoveFocus", axis: "rows", by: "page", forward: true, extend: true },
  { type: "MoveFocus", axis: "cols", by: "edge", forward: false, extend: false },
  { type: "ScrollBy", axis: "rows", items: -3 },
  { type: "Hide", axis: "cols" },
  { type: "ShowHidden", axis: "rows" },
];

const platforms = gs.sampledFrom<Platform>(["mac", "other"]);
const actions = gs.sampledFrom(ACTIONS);
/** Named keys, or any character, which a chord names as itself. */
const keys = gs.oneOf(
  gs.sampledFrom(KEYS),
  gs.characters().map((c) => ({ event: c, names: [c] as readonly string[] })),
);
const modifierSets = gs.sets(gs.sampledFrom(MODIFIERS)).map((s) => new Set(s));

/** The key event for pressing `key` with exactly `held`. A letter typed with Shift reports its capital. */
function press(key: string, held: Set<Modifier>): KeyChord {
  const shifted = held.has("shift") && /^[a-z]$/.test(key) ? key.toUpperCase() : key;
  return {
    key: shifted,
    ctrlKey: held.has("ctrl"),
    altKey: held.has("alt"),
    shiftKey: held.has("shift"),
    metaKey: held.has("meta"),
  };
}

/** `held` and `key` written as a chord, modifiers in any order and spelling. */
function spell(tc: hegel.TestCase, held: Set<Modifier>, names: readonly string[]): string {
  const parts = [...held].map((m) => tc.draw(gs.sampledFrom(SPELLINGS[m])));
  const order = tc.draw(gs.arrays(gs.integers(), { minSize: parts.length, maxSize: parts.length }));
  const shuffled = parts.map((p, i) => [order[i]!, p] as const).sort((a, b) => a[0] - b[0]).map(([, p]) => p);
  return [...shuffled, tc.draw(gs.sampledFrom(names))].join("+");
}

test("a bound chord is found from its key event, however the chord was written", () =>
  hegel.test((tc) => {
    const platform = tc.draw(platforms);
    const held = tc.draw(modifierSets);
    const key = tc.draw(keys);
    const action = tc.draw(actions);
    const chord = spell(tc, held, key.names);
    tc.note(`chord ${chord}`);
    const keymap = createKeymap({ platform, defaults: false, bindings: { [chord]: action } });
    expect(keymap.lookup(press(key.event, held))).toEqual(action);
  }));

test("a chord matches only its exact modifiers", () =>
  hegel.test((tc) => {
    const platform = tc.draw(platforms);
    const held = tc.draw(modifierSets);
    const other = tc.draw(modifierSets);
    tc.assume([...held].sort().join() !== [...other].sort().join());
    const key = tc.draw(keys);
    const keymap = createKeymap({ platform, defaults: false, bindings: { [spell(tc, held, key.names)]: { type: "Cancel" } } });
    expect(keymap.lookup(press(key.event, other))).toBeUndefined();
  }));

test("Mod is Cmd on macOS and Ctrl elsewhere", () =>
  hegel.test((tc) => {
    const platform = tc.draw(platforms);
    const key = tc.draw(keys);
    const extra = tc.draw(gs.sets(gs.sampledFrom<Modifier>(["alt", "shift"])));
    const mod: Modifier = platform === "mac" ? "meta" : "ctrl";
    const not: Modifier = platform === "mac" ? "ctrl" : "meta";
    const chord = ["Mod", ...[...extra].map((m) => SPELLINGS[m][0]!), tc.draw(gs.sampledFrom(key.names))].join("+");
    const keymap = createKeymap({ platform, defaults: false, bindings: { [chord]: { type: "SelectAll" } } });
    expect(keymap.lookup(press(key.event, new Set([...extra, mod])))).toEqual({ type: "SelectAll" });
    expect(keymap.lookup(press(key.event, new Set([...extra, not])))).toBeUndefined();
  }));

test("the widget user's bindings replace the defaults, and null unbinds", () =>
  hegel.test((tc) => {
    const platform = tc.draw(platforms);
    const held = tc.draw(modifierSets);
    const key = tc.draw(keys);
    const action = tc.draw(gs.optional(actions));
    const chord = spell(tc, held, key.names);
    const keymap = createKeymap({ platform, bindings: { [chord]: action } });
    expect(keymap.lookup(press(key.event, held))).toEqual(action ?? undefined);
  }));

test("the defaults leave other chords as they were", () =>
  hegel.test((tc) => {
    const platform = tc.draw(platforms);
    const held = tc.draw(modifierSets);
    const key = tc.draw(keys);
    const bound = tc.draw(modifierSets);
    tc.assume([...held].sort().join() !== [...bound].sort().join());
    const plain = createKeymap({ platform });
    const extended = createKeymap({ platform, bindings: { [spell(tc, bound, key.names)]: { type: "Cancel" } } });
    expect(extended.lookup(press(key.event, held))).toEqual(plain.lookup(press(key.event, held)));
  }));

test("by default, characters typed alone or with Shift and Tab are not the grid's", () =>
  hegel.test((tc) => {
    const platform = tc.draw(platforms);
    const shift = tc.draw(gs.booleans());
    const key = tc.draw(gs.oneOf(gs.characters({ codec: "utf-8" }), gs.just("Tab")));
    tc.assume(key !== " ");
    const keymap = createKeymap({ platform });
    expect(keymap.lookup(press(key, new Set<Modifier>(shift ? ["shift"] : [])))).toBeUndefined();
  }));

test.each<[Platform, KeyChord, Action]>([
  ["mac", { key: "ArrowDown", ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }, { type: "MoveFocus", axis: "rows", by: "item", forward: true, extend: false }],
  ["other", { key: "ArrowLeft", ctrlKey: false, altKey: false, shiftKey: true, metaKey: false }, { type: "MoveFocus", axis: "cols", by: "item", forward: false, extend: true }],
  ["mac", { key: "ArrowRight", ctrlKey: false, altKey: false, shiftKey: true, metaKey: true }, { type: "MoveFocus", axis: "cols", by: "edge", forward: true, extend: true }],
  ["other", { key: "ArrowUp", ctrlKey: true, altKey: false, shiftKey: false, metaKey: false }, { type: "MoveFocus", axis: "rows", by: "edge", forward: false, extend: false }],
  ["other", { key: "PageDown", ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }, { type: "MoveFocus", axis: "rows", by: "page", forward: true, extend: false }],
  ["mac", { key: "End", ctrlKey: false, altKey: false, shiftKey: true, metaKey: false }, { type: "MoveFocus", axis: "rows", by: "edge", forward: true, extend: true }],
  ["mac", { key: "a", ctrlKey: false, altKey: false, shiftKey: false, metaKey: true }, { type: "SelectAll" }],
  ["other", { key: "a", ctrlKey: true, altKey: false, shiftKey: false, metaKey: false }, { type: "SelectAll" }],
  ["mac", { key: " ", ctrlKey: true, altKey: false, shiftKey: false, metaKey: false }, { type: "SelectColumn" }],
  ["other", { key: " ", ctrlKey: false, altKey: false, shiftKey: true, metaKey: false }, { type: "SelectRow" }],
  ["mac", { key: "PageDown", ctrlKey: false, altKey: true, shiftKey: false, metaKey: false }, { type: "MoveFocus", axis: "cols", by: "page", forward: true, extend: false }],
  ["other", { key: "PageUp", ctrlKey: false, altKey: true, shiftKey: true, metaKey: false }, { type: "MoveFocus", axis: "cols", by: "page", forward: false, extend: true }],
  ["other", { key: "Home", ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }, { type: "MoveFocus", axis: "rows", by: "edge", forward: false, extend: false }],
  ["other", { key: "Escape", ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }, { type: "Cancel" }],
  ["other", { key: "Enter", ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }, { type: "Activate" }],
  ["mac", { key: "F2", ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }, { type: "BeginEdit" }],
  ["mac", { key: "ArrowDown", ctrlKey: false, altKey: true, shiftKey: false, metaKey: false }, { type: "OpenColumnMenu" }],
  ["other", { key: "ArrowDown", ctrlKey: false, altKey: true, shiftKey: false, metaKey: false }, { type: "OpenColumnMenu" }],
  ["mac", { key: "ArrowRight", ctrlKey: false, altKey: true, shiftKey: false, metaKey: false }, { type: "OpenRowMenu" }],
  ["other", { key: "ArrowRight", ctrlKey: false, altKey: true, shiftKey: false, metaKey: false }, { type: "OpenRowMenu" }],
  ["mac", { key: "ArrowLeft", ctrlKey: false, altKey: true, shiftKey: true, metaKey: false }, { type: "Pin", axis: "cols", edge: "leading" }],
  ["other", { key: "ArrowLeft", ctrlKey: false, altKey: true, shiftKey: true, metaKey: false }, { type: "Pin", axis: "cols", edge: "leading" }],
  ["mac", { key: "Backspace", ctrlKey: false, altKey: true, shiftKey: true, metaKey: false }, { type: "Unpin", axis: "cols" }],
  ["other", { key: "F10", ctrlKey: false, altKey: false, shiftKey: true, metaKey: false }, { type: "OpenColumnMenu" }],
  ["mac", { key: "ContextMenu", ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }, { type: "OpenColumnMenu" }],
  ["other", { key: "0", code: "Digit0", ctrlKey: true, altKey: true, shiftKey: false, metaKey: false }, { type: "Hide", axis: "cols" }],
  ["other", { key: "9", code: "Digit9", ctrlKey: true, altKey: true, shiftKey: false, metaKey: false }, { type: "Hide", axis: "rows" }],
  // macOS gives Option+0 the key "º", and Shift+0 ")": the physical key finds the binding.
  ["mac", { key: "º", code: "Digit0", ctrlKey: false, altKey: true, shiftKey: false, metaKey: true }, { type: "Hide", axis: "cols" }],
  ["mac", { key: "‚", code: "Digit0", ctrlKey: false, altKey: true, shiftKey: true, metaKey: true }, { type: "ShowHidden", axis: "cols" }],
  ["other", { key: "(", code: "Digit9", ctrlKey: true, altKey: true, shiftKey: true, metaKey: false }, { type: "ShowHidden", axis: "rows" }],
  ["mac", { key: "ArrowRight", ctrlKey: false, altKey: true, shiftKey: true, metaKey: false }, { type: "Pin", axis: "cols", edge: "trailing" }],
  ["other", { key: "ArrowUp", ctrlKey: false, altKey: true, shiftKey: true, metaKey: false }, { type: "Pin", axis: "rows", edge: "leading" }],
  ["mac", { key: "ArrowDown", ctrlKey: false, altKey: true, shiftKey: true, metaKey: false }, { type: "Pin", axis: "rows", edge: "trailing" }],
  ["mac", { key: "Backspace", ctrlKey: false, altKey: true, shiftKey: true, metaKey: true }, { type: "Unpin", axis: "rows" }],
  ["other", { key: "Backspace", ctrlKey: true, altKey: true, shiftKey: true, metaKey: false }, { type: "Unpin", axis: "rows" }],
  ["mac", { key: "Home", ctrlKey: false, altKey: true, shiftKey: true, metaKey: false }, { type: "Freeze", axis: "cols" }],
  ["other", { key: "Home", ctrlKey: true, altKey: true, shiftKey: true, metaKey: false }, { type: "Freeze", axis: "rows" }],
])("default on %s: %o is %o", (platform, event, action) => {
  expect(createKeymap({ platform }).lookup(event)).toEqual(action);
});

test("the physical key finds a binding only when the key itself names none", () => {
  const keymap = createKeymap({ platform: "other", bindings: { "Alt+º": { type: "Cancel" }, "Alt+Q": null } });
  const alt = { ctrlKey: false, altKey: true, shiftKey: false, metaKey: false };
  expect(keymap.lookup({ ...alt, key: "º", code: "Digit0" })).toEqual({ type: "Cancel" });
  expect(keymap.lookup({ ...alt, key: "œ", code: "KeyQ" })).toBeUndefined();
  expect(keymap.lookup({ ...alt, key: "ArrowDown", code: "ArrowDown" })).toEqual({ type: "OpenColumnMenu" });
  expect(keymap.lookup({ ...alt, key: "∂", code: "KeyD" })).toBeUndefined();
});

test("Ctrl+A is not select-all on macOS, where Ctrl+A moves to the line start", () => {
  const event = { key: "a", ctrlKey: true, altKey: false, shiftKey: false, metaKey: false };
  expect(createKeymap({ platform: "mac" }).lookup(event)).toBeUndefined();
});

test("a chord naming an unknown modifier or no key is rejected", () => {
  expect(() => createKeymap({ bindings: { "Hyper+a": { type: "Cancel" } } })).toThrow(/Hyper/);
  expect(() => createKeymap({ bindings: { "Ctrl+": { type: "Cancel" } } })).toThrow();
});
