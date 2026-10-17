# @flotilla-org/binnacle

The binnacle Grid on a web page. The Grid runs as wasm on the main thread
and paints onto a canvas inside a native scroller, so it scrolls tables of
any size (up to 2^53 - 1 rows). Your data stays in JavaScript behind one
interface, the Table Source, whose answers may arrive at once or later.

- `@flotilla-org/binnacle`: the framework-neutral `GridWidget`, the Table
  Source types, `fromRows` (a ready-made in-memory source), Themes and
  Border Styles.
- `@flotilla-org/binnacle/react`: `BinnacleGrid`, a thin React wrapper,
  plus everything the main entry exports. React is an optional peer
  dependency (18 or later).

## Install from the repo

The package is not published to npm. It ships its wasm build inside it, and
both the wasm and the JavaScript in `dist/` are build outputs, not committed,
so build it once from a checkout (needs Rust with the `wasm32-unknown-unknown`
target, `wasm-pack` and pnpm):

```sh
git clone https://github.com/flotilla-org/binnacle
pnpm -C binnacle/web install
pnpm -C binnacle/web build
```

Then depend on it from your app, either as a member of the same pnpm
workspace (`"@flotilla-org/binnacle": "workspace:*"`, as `consumer/` here
does) or by path (`"@flotilla-org/binnacle": "link:../binnacle/web"`).
Rebuild after pulling.

The wasm loads through `new URL(..., import.meta.url)`, which Vite and
webpack 5 emit as an asset with no configuration (`consumer/` checks Vite).
esbuild leaves that URL alone: copy `wasm/binnacle_wasm_bg.wasm` beside your
bundle and call `loadBinnacle(new URL("binnacle_wasm_bg.wasm", location.href))`
before creating a grid. The module loads once, on the first `loadBinnacle()`
or `GridWidget.create`.

The wasm-bindgen types use `Symbol.dispose`: with `skipLibCheck` off, add
`"esnext.disposable"` to your `lib`.

## Use

```ts
import { GridWidget, fromRows } from "@flotilla-org/binnacle";

const rows = [
  { name: "Endurance", tonnage: 348 },
  { name: "Fram", tonnage: 402 },
];
const source = fromRows(rows, [{ key: "name" }, { key: "tonnage", label: "Tonnage" }]);
const grid = await GridWidget.create(document.querySelector("#grid")!, {
  source,
  onFrame: (frame) => console.log(frame.focusIds), // { row: 1, col: "tonnage" }
});

// Later: stop the frame loop and free the wasm side.
grid.destroy();
source.destroy();
```

The grid fills its container, so give the container a size. Other options:
`rowHeight`, `columnWidth`, `headerHeight`, `theme` (a Theme, or a function
from `"light" | "dark"` to one; it follows the system by default), `border`
(a Border Style), `keymap`, `platform`, `renderer` (a Cell Renderer per
cell, for example `byColumn(...)` over `textRenderer`, `sparklineRenderer`
and `barRenderer`), `onCommit` (makes cells editable; without it the grid is
read-only) and `editor` (a custom editor per cell). `grid.theme` and
`grid.border` change them later.

### React

```tsx
import { fromRows } from "@flotilla-org/binnacle";
import { BinnacleGrid } from "@flotilla-org/binnacle/react";
import { useEffect, useMemo, useState } from "react";

function Ships({ rows }: { rows: Ship[] }) {
  const source = useMemo(() => fromRows(rows, [{ key: "name" }, { key: "tonnage" }]), [rows]);
  const [focus, setFocus] = useState<{ row: number; col: keyof Ship } | null>(null);
  useEffect(() => () => source.destroy(), [source]);
  return <BinnacleGrid source={source} style={{ height: 400 }} onFrame={(f) => setFocus(f.focusIds)} />;
}
```

`BinnacleGrid` takes every `GridWidget` option as a prop, plus `className`,
`style` and `widgetRef` (receives the `GridWidget`, then `null`). The widget
runs its own frame loop: React renders the container once and never on
frames. Props reach it like this:

- Function options (`onFrame`, `onCommit`, `editor`, `renderer`) are always
  called as they are in the latest render, so inline closures are fine.
  Adding or removing one makes a new widget (no `onCommit` means read-only).
  A new `renderer` draws from the next frame the grid paints; it does not
  force one.
- `theme` and `border` restyle the same widget.
- Any other option is read when the widget is created; a new value (a new
  `source`, a new `keymap` object) makes a new widget. Memoize them.

The wrapper does not destroy your Table Source: you made it, you free it.

## The Table Source

A Table Source answers for order and values, per axis, in your own ID and
value types. Every method may return a value or a Promise; late answers are
fine, stale ones (from before a Change Notice) are dropped.

```ts
type MaybePromise<T> = T | Promise<T>;

interface TableSource<RowId, ColId, V = unknown> {
  rows: AxisSource<RowId, ColId>;
  cols: AxisSource<ColId, RowId>;
  /** The values where `rows` and `cols` cross, one array per row. */
  cells(rows: RowId[], cols: ColId[], signal: AbortSignal): MaybePromise<V[][]>;
  /** Hands the source its sinks for Change Notices; returns how to detach. */
  attach?(sink: { rows: AxisSink<RowId>; cols: AxisSink<ColId> }): () => void;
}

interface AxisSource<Id, CrossId> {
  count(): MaybePromise<number>;
  /** The IDs at Positions `start..end`. */
  window(start: number, end: number, signal: AbortSignal): MaybePromise<Id[]>;
  /** Sorts by `spec` (columns, for the rows axis) and returns the spec now in force. */
  sort?(spec: SortSpec<CrossId>, signal: AbortSignal): MaybePromise<SortSpec<CrossId>>;
  sortable?(by: CrossId): boolean;
  /** Where `id` sits after a reorder: `{ type: "Found", position }` or `{ type: "Gone", suggest }`. */
  locate?(id: Id, signal: AbortSignal): MaybePromise<Located>;
  /** Moves `ids` before Position `to`; `false` refuses. */
  move?(ids: Id[], to: number): MaybePromise<boolean>;
  /** Header text; `String(id)` for columns and Position + 1 for rows by default. */
  label?(id: Id, position: number): string;
  /** Required when `Id` is not a string, number or bigint: its primitive form. */
  idKey?: (id: Id) => string | number | bigint;
  /** The axis's IDs are its Positions (huge or position-only tables). */
  identity?: "position";
}
```

A source tells the grid what changed through the sink:
`sink.rows.notify({ type: "Reset" })`, or `Reordered`, `Inserted { at, n }`,
`Removed { at, n }`, `ContentChanged { start, end }`. Filtering is not part of
the interface: filter however you like, then notify (`fromRows` has
`filter(keep)` for this). A sort the source applies arrives as a reorder
through the sink too.

`fromRows(rows, columns)` covers arrays of objects: row IDs are array
indices, column IDs property names, every column sorts unless
`sortable: false`, and the rows stay in JavaScript.

## Develop

`pnpm dev` serves the demo on http://127.0.0.1:5754 (the React page is
`/react.html`). `pnpm typecheck`, `pnpm test` (vitest and the wasm tests) and
`pnpm e2e` (Playwright, Chromium and Firefox; builds the package and the
`consumer/` fixture first) are the gates.
