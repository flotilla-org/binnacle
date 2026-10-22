// A slow aggregate backend for groupedSource, standing in for a remote
// service: it works each Aggregate Query out from a fromRows table and
// answers it late, groups and totals after one delay and Leaves after a
// longer one, so pending groups and Leaves show before they fill. The table
// has already filtered and sorted its rows as the View Spec asks (groupedSource
// gives it the rows' part), so the backend reads its Leaves from there,
// groups them by each level column's cell text and orders siblings by the
// one sort rule, as fromRows does. A query a newer View Spec superseded is
// never answered.

import type { AggregateBackend, AggregateBatch, AggregateQuery, Figure, GroupPath, GroupRow, ListedValue, RowsColId, RowsSource } from "../../src";

/** How late the backend answers, ms: groups and totals, then Leaves. */
export const AGGREGATE_PACES = {
  brisk: { groups: 150, leaves: 400 },
  slow: { groups: 500, leaves: 1500 },
  glacial: { groups: 1500, leaves: 4000 },
} as const;

export type AggregatePace = keyof typeof AGGREGATE_PACES;

/** A value as the level columns group it: its text, `null` for a blank. */
const text = (v: unknown): ListedValue =>
  v === null || v === undefined ? null : v instanceof Date ? v.toISOString().slice(0, 10) : String(v);
const numeric = (v: unknown) => (typeof v === "number" ? v : v instanceof Date ? v.getTime() : null);

/** Ascending text, the blank last. */
const byText = (a: ListedValue, b: ListedValue) => (a === b ? 0 : a === null ? 1 : b === null ? -1 : a < b ? -1 : 1);

export interface SlowBackend<R> extends AggregateBackend<keyof R & string> {
  /** How many queries wait for their answer. */
  readonly inFlight: number;
}

/** A backend over `table`'s rows (`rows[id]` its Leaf `id`), answering at `pace()`; `changed` hears every query asked or answered. */
export function slowBackend<R extends object, C extends RowsColId<R>>(
  table: RowsSource<R, C>,
  rows: readonly R[],
  pace: () => AggregatePace,
  changed: () => void,
): SlowBackend<R> {
  type Field = keyof R & string;
  let inFlight = 0;

  const compute = (batch: AggregateBatch<Field>, q: AggregateQuery<Field>) => {
    const { levels, values, sort } = batch.scope;
    const signal = new AbortController().signal;
    const ids = table.rows.window(0, table.rows.count() as number, signal) as number[];
    const value = (id: number, field: Field) => rows[id]![field];
    const under = (path: GroupPath<Field>) => ids.filter((id) => path.every((s) => text(value(id, s.field)) === s.value));
    const figures = (leaves: number[]): Figure[] =>
      values.map((v) => {
        if (v.field === null) return v.aggregate === "count" ? leaves.length : null;
        const field = v.field;
        const cells = leaves.map((id) => value(id, field)).filter((c) => c !== null && c !== undefined);
        if (v.aggregate === "count") return cells.length;
        if (v.aggregate === "distinct count") return new Set(cells.map(text)).size;
        const ns = cells.map(numeric).filter((n): n is number => n !== null);
        if (ns.length === 0) return null;
        const sum = ns.reduce((a, b) => a + b, 0);
        return { sum, average: sum / ns.length, minimum: Math.min(...ns), maximum: Math.max(...ns) }[v.aggregate] ?? null;
      });
    const groups = (path: GroupPath<Field>): GroupRow[] => {
      const field = levels[path.length]!;
      const leaves = under(path);
      const found = [...new Set(leaves.map((id) => text(value(id, field))))].map((v): GroupRow => {
        const inner = under([...path, { field, value: v }]);
        const next = levels[path.length + 1];
        const children = next === undefined ? inner.length : new Set(inner.map((id) => text(value(id, next)))).size;
        return { value: v, children, figures: figures(inner) };
      });
      // The one sort rule: by this level's column, by a column a value
      // aggregates (its figure), else ascending.
      const rule = sort.find((r) => r.id === field || values.some((v) => v.field === r.id));
      const flip = rule?.direction === "descending" ? -1 : 1;
      const at = rule && rule.id !== field ? values.findIndex((v) => v.field === rule.id) : -1;
      return found.sort((a, b) => {
        if (at < 0) return a.value === null || b.value === null ? byText(a.value, b.value) : flip * byText(a.value, b.value);
        const x = a.figures[at] as number | null;
        const y = b.figures[at] as number | null;
        return x === y ? byText(a.value, b.value) : x === null ? 1 : y === null ? -1 : flip * (x - y);
      });
    };
    const answer = { stamp: batch.stamp, id: q.id, path: q.path, start: q.start };
    if (q.want === "total") return { ...answer, body: { type: "total" as const, figures: figures(under(q.path)) } };
    if (q.want === "groups") {
      const all = groups(q.path);
      return { ...answer, body: { type: "groups" as const, total: all.length, groups: all.slice(q.start, q.end) } };
    }
    if (q.want === "leaves") return { ...answer, body: { type: "leaves" as const, ids: under(q.path).slice(q.start, q.end) } };
    const id = q.want.locate;
    if (!ids.includes(id)) return { ...answer, path: [], start: 0, body: { type: "located" as const, id, found: false } };
    const path = levels.map((field) => ({ field, value: text(value(id, field)) }));
    return { ...answer, path, start: under(path).indexOf(id), body: { type: "located" as const, id, found: true } };
  };

  return {
    get inFlight() {
      return inFlight;
    },
    answer(batch, reply, signal) {
      for (const q of batch.queries) {
        inFlight++;
        const timer = setTimeout(
          () => {
            signal.removeEventListener("abort", stop);
            inFlight--;
            reply(compute(batch, q));
            changed();
          },
          AGGREGATE_PACES[pace()][q.want === "leaves" ? "leaves" : "groups"],
        );
        const stop = () => {
          clearTimeout(timer);
          inFlight--;
          changed();
        };
        signal.addEventListener("abort", stop, { once: true });
      }
      changed();
    },
  };
}
