// The one place TypeScript encodes small enums as the integers the wasm
// inputs take. `crates/binnacle-wasm/src/codes.rs` is the other side.

export type AxisName = "rows" | "cols";

export const AXIS_CODE: Record<AxisName, number> = { rows: 0, cols: 1 };

export const AXES: readonly AxisName[] = ["rows", "cols"];

/** Change Notice types, as the wasm `notify` input takes them. */
export const NOTICE_CODE = { Reordered: 0, Reset: 1, Inserted: 2, Removed: 3, ContentChanged: 4, Refiltered: 5 } as const;

export const POINTER = { down: 0, move: 1, up: 2 } as const;

/** The modifier bitmask the wasm `pointer` input takes. */
export function modifierBits(e: { shiftKey: boolean; ctrlKey: boolean; altKey: boolean; metaKey: boolean }): number {
  return (e.shiftKey ? 1 : 0) | (e.ctrlKey ? 2 : 0) | (e.altKey ? 4 : 0) | (e.metaKey ? 8 : 0);
}

export const FOCUS_STEP = { item: 0, page: 1, edge: 2 } as const;

export const EDIT_END = { commit: 0, cancel: 1 } as const;

export const SELECTION_UNIT = { cells: 0, rows: 1 } as const;

/** The support bits the wasm `set_support` input takes; `textRegex` needs `text`. */
export const SUPPORT = { sort: 1, text: 2, textRegex: 4 } as const;

/** The quick filter bits the wasm `set_quick_filter_support` input takes. */
export const QUICK_FILTER = { on: 1, regex: 2 } as const;
