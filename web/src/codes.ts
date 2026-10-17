// The one place TypeScript encodes small enums as the integers the wasm
// inputs take. `crates/binnacle-wasm/src/codes.rs` is the other side.

export type AxisName = "rows" | "cols";

export const AXIS_CODE: Record<AxisName, number> = { rows: 0, cols: 1 };

export const AXES: readonly AxisName[] = ["rows", "cols"];
