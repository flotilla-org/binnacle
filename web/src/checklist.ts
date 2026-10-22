// The list-of-values checklist's rules (#107): which conditions on a column
// a tick writes. A capped list can't be written out, so each change writes
// whichever condition is exact: unticking from all gives `not OneOf`, ticking
// from none gives `OneOf`, and "select all matching" a search gives a Text
// condition on it, which later unticks AND a `not OneOf` onto. The ticks are
// read back from the conditions against the values shown.
//
// The checklist owns a column's first Text condition and its first OneOf;
// any others stay as they are. A value a Text condition rejects can't be
// ticked: the menu shows it disabled.

import type { Condition, ListedValue, TextMatch } from "./source";

/**
 * The conditions on one column the column menu reads and writes: its first
 * Text condition and its first OneOf (the checklist's), and its first
 * comparison.
 */
export interface ColumnConditions<ColId> {
  text: Condition<ColId> | null;
  oneOf: Condition<ColId> | null;
  compare: Condition<ColId> | null;
  /** Every condition on the column, those two included, in order. */
  all: Condition<ColId>[];
}

/** The values the checklist shows: those the source listed, the blank as `null` first if listed, and whether there were more. */
export interface Shown {
  values: ListedValue[];
  partial: boolean;
  /** The checklist's search, empty for none. */
  search: string;
}

/** The conditions on column `id` among `conditions`. */
export function columnConditions<ColId>(conditions: Condition<ColId>[], id: ColId): ColumnConditions<ColId> {
  const all = conditions.filter((c) => c.id === id);
  return {
    text: all.find((c) => c.kind.type === "Text") ?? null,
    oneOf: all.find((c) => c.kind.type === "OneOf") ?? null,
    compare: all.find((c) => c.kind.type === "Compare") ?? null,
    all,
  };
}

/**
 * Whether a Text query matches `value` (never a blank), read here as fromRows
 * reads it: lowercased substrings, or a `RegExp`; `undefined` for a pattern
 * `RegExp` rejects.
 */
function textMatches(query: TextMatch, value: ListedValue): boolean | undefined {
  if (!query.regex) {
    if (value === null) return false;
    return query.caseSensitive ? value.includes(query.text) : value.toLowerCase().includes(query.text.toLowerCase());
  }
  try {
    const pattern = new RegExp(query.text, query.caseSensitive ? "u" : "iu");
    return value !== null && pattern.test(value);
  } catch {
    return undefined;
  }
}

/**
 * Whether `condition` keeps an item whose value is `value` (`null` for a
 * blank); one with an invalid pattern, which no source applies, keeps
 * everything, as does a comparison, which a listed value's text can't
 * answer, and a Native condition, which only its source reads.
 */
export function keeps<ColId>(condition: Condition<ColId>, value: ListedValue): boolean {
  const kind = condition.kind;
  if (kind.type === "Compare" || kind.type === "Native") return true;
  const matched = kind.type === "OneOf" ? kind.values.includes(value) : textMatches(kind.query, value);
  return matched === undefined || matched !== condition.not;
}

/** Whether the column's Text conditions reject `value`, so it can't be ticked. */
export function excluded<ColId>(column: ColumnConditions<ColId>, value: ListedValue): boolean {
  return column.all.some((c) => c.kind.type === "Text" && !keeps(c, value));
}

/** Whether `value` is ticked: every condition on the column keeps it. */
export function ticked<ColId>(column: ColumnConditions<ColId>, value: ListedValue): boolean {
  return column.all.every((c) => keeps(c, value));
}

/**
 * The conditions the column menu writes in place of the column's Text
 * condition and OneOf, and its comparison when `compare` is given.
 */
export interface Written<ColId> {
  text: Condition<ColId> | null;
  oneOf: Condition<ColId> | null;
  compare?: Condition<ColId> | null;
}

/** Each condition `written` replaces on `column`, and what replaces it (`null` to remove it). */
export function writes<ColId>(column: ColumnConditions<ColId>, written: Written<ColId>) {
  const pairs: [Condition<ColId> | null, Condition<ColId> | null][] = [
    [column.text, written.text],
    [column.oneOf, written.oneOf],
  ];
  if (written.compare !== undefined) pairs.push([column.compare, written.compare]);
  return pairs;
}

/**
 * Ticks (`on`) or unticks `value`. With no OneOf, unticking writes
 * `not OneOf([value])`; a OneOf lists the ticked values, a `not OneOf` the
 * unticked ones. A `not OneOf` left with none, or a OneOf holding every value
 * shown of a whole unsearched list, is removed.
 */
export function tick<ColId>(
  column: ColumnConditions<ColId>,
  id: ColId,
  value: ListedValue,
  on: boolean,
  shown: Shown,
): Written<ColId> {
  const { text, oneOf } = column;
  if (excluded(column, value)) return { text, oneOf };
  if (!oneOf || oneOf.kind.type !== "OneOf") {
    return { text, oneOf: on ? null : { id, kind: { type: "OneOf", values: [value] }, not: true } };
  }
  const listed = oneOf.kind.values.filter((v) => v !== value);
  // A OneOf lists the ticked values; a `not OneOf` the unticked ones.
  if (on !== oneOf.not) listed.push(value);
  if (oneOf.not && listed.length === 0) return { text, oneOf: null };
  const whole = !shown.partial && shown.search === "";
  if (!oneOf.not && whole && shown.values.every((v) => excluded(column, v) || listed.includes(v))) {
    return { text, oneOf: null };
  }
  return { text, oneOf: { id, kind: { type: "OneOf", values: listed }, not: oneOf.not } };
}

/**
 * The head of the list: "(Select all)", or "(Select all matching …)" while
 * searching. Ticked when every value shown that can be ticked is, or for a
 * search when the column's Text condition is that search and no OneOf
 * narrows it.
 */
export function headTicked<ColId>(column: ColumnConditions<ColId>, shown: Shown): boolean {
  if (shown.search !== "") {
    const q = column.text?.kind.type === "Text" ? column.text.kind.query : null;
    const searched = q !== null && !column.text!.not && q.text === shown.search && !q.regex && !q.caseSensitive;
    return searched && column.oneOf === null;
  }
  const open = shown.values.filter((v) => !excluded(column, v));
  return open.length > 0 && open.every((v) => ticked(column, v));
}

/**
 * Ticks (`on`) or unticks the head. Without a search, ticking removes the
 * OneOf and unticking writes `OneOf([])`, so later ticks start from none.
 * With one, ticking writes a Text condition on the search in place of the
 * column's (dropping the OneOf), and unticking unticks each value shown.
 */
export function tickHead<ColId>(column: ColumnConditions<ColId>, id: ColId, on: boolean, shown: Shown): Written<ColId> {
  if (shown.search === "") {
    return { text: column.text, oneOf: on ? null : { id, kind: { type: "OneOf", values: [] }, not: false } };
  }
  if (on) {
    const query: TextMatch = { text: shown.search, caseSensitive: false, regex: false };
    return { text: { id, kind: { type: "Text", query }, not: false }, oneOf: null };
  }
  let now = column;
  for (const value of shown.values) {
    const written = tick(now, id, value, false, shown);
    now = { text: written.text, oneOf: written.oneOf, compare: now.compare, all: replaced(now, written) };
  }
  return { text: now.text, oneOf: now.oneOf };
}

/** The column's conditions with `written` in place of its Text condition and OneOf (each added at the end if new). */
export function replaced<ColId>(column: ColumnConditions<ColId>, written: Written<ColId>): Condition<ColId>[] {
  let all = [...column.all];
  for (const [from, to] of writes(column, written)) {
    const i = from ? all.indexOf(from) : -1;
    if (i >= 0 && to) all[i] = to;
    else if (i >= 0) all = all.filter((_, j) => j !== i);
    else if (to) all.push(to);
  }
  return all;
}
