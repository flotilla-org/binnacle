// The grouping menu's items, for the column menu and the level menu. From a
// column of the table: "Group rows by this column", "Add as a column level"
// and, while grouped, "Add a value ▸" with the aggregates the source offers
// over it. From a level (a pivot's column names its innermost column level,
// a corner name its own): "Remove level", "Move to rows" or "Move to
// columns", "Collapse to" and "Expand through" it, then "Remove value" for
// each value while there are two or more, "Add a value ▸", and "Group by
// another column ▸" with the fields no level uses. Each item is offered only
// where the source supports it; aggregates that take parameters are left
// out, as the menu has nowhere to ask for them. A "▸" item opens its list in
// place.

import type { AxisName } from "./codes";
import { valueName, type AggregateValue } from "./source";
import type { GridWidget } from "./widget";

/**
 * What a grouping menu is for: a column of the table, or a level of `axis`
 * (`null` for the values pseudo-level; `undefined` for a pivot's Grand Total,
 * which names none).
 */
export type GroupingTarget<ColId> = { column: ColId } | { axis: AxisName; level: ColId | null | undefined };

const LIST_STYLE = "display: flex; flex-direction: column; gap: 2px; padding-left: 12px; max-height: 200px; overflow: auto;";

/**
 * The grouping menu's items for `target`, each built by `item` (a menu
 * button) and running `done` (which closes the menu) before its edit.
 */
export function groupingItems<RowId, ColId, V>(
  widget: GridWidget<RowId, ColId, V>,
  target: GroupingTarget<ColId>,
  item: (text: string, className: string) => HTMLButtonElement,
  done: () => void,
): HTMLElement[] {
  const label = (id: ColId) => widget.columnLabel(id);
  const rows = widget.levels("rows");
  const cols = widget.levels("cols");
  const used = (id: ColId) => rows.includes(id) || cols.includes(id);
  const grouped = rows.length + cols.length > 0;
  const values = widget.values();
  const same = (a: AggregateValue<ColId>, b: AggregateValue<ColId>) =>
    a.field === b.field && a.aggregate === b.aggregate && (a.params ?? []).join("\u0000") === (b.params ?? []).join("\u0000");
  const name = (v: AggregateValue<ColId>) => valueName(v, label);
  /** A line between sections. */
  const rule = () => {
    const line = document.createElement("div");
    line.setAttribute("role", "separator");
    line.style.cssText = `height: 1px; margin: 4px -8px; background: ${widget.theme.border};`;
    return line;
  };

  const action = (text: string, className: string, run: () => void) => {
    const b = item(text, className);
    b.addEventListener("click", () => {
      done();
      run();
    });
    return b;
  };
  /** An item that opens `items` in a list under it. */
  const disclosure = (text: string, className: string, items: HTMLElement[]) => {
    if (items.length === 0) return [];
    const b = item(`${text} ▸`, className);
    b.setAttribute("aria-expanded", "false");
    const list = document.createElement("div");
    list.className = `${className}-list`;
    list.style.cssText = LIST_STYLE;
    list.setAttribute("role", "group");
    list.setAttribute("aria-label", text);
    list.hidden = true;
    list.append(...items);
    b.addEventListener("click", () => {
      list.hidden = !list.hidden;
      // Inline styles would win over the `hidden` attribute.
      list.style.display = list.hidden ? "none" : "flex";
      b.setAttribute("aria-expanded", String(!list.hidden));
      b.textContent = `${text} ${list.hidden ? "▸" : "▾"}`;
      if (!list.hidden) (list.firstElementChild as HTMLElement | null)?.focus();
    });
    list.style.display = "none";
    return [b, list];
  };

  /** The values offered over `field` not yet figured, as add buttons. */
  const addable = (field: ColId) =>
    widget
      .columnSupport(field)
      .aggregates.filter((a) => (a.params ?? []).length === 0)
      .map((a): AggregateValue<ColId> => ({ field, aggregate: a.name }))
      .filter((v) => !values.some((w) => same(v, w)))
      .map((v) => action(name(v), "binnacle-grouping-add-value", () => widget.addValue(v)));

  if ("column" in target) {
    const id = target.column;
    const support = widget.columnSupport(id);
    return [
      ...(support.group && !used(id) ? [action("Group rows by this column", "binnacle-column-menu-group", () => widget.groupBy(id))] : []),
      ...(support.pivot && !used(id)
        ? [action("Add as a column level", "binnacle-grouping-add-column-level", () => widget.groupBy(id, "cols"))]
        : []),
      ...(grouped ? disclosure("Add a value", "binnacle-grouping-values", addable(id)) : []),
    ];
  }

  const { axis, level } = target;
  const levels = axis === "rows" ? rows : cols;
  const at = level === undefined ? -1 : levels.indexOf(level);
  const other: AxisName = axis === "rows" ? "cols" : "rows";
  const items: HTMLElement[] = [];
  if (level != null && at >= 0) {
    const support = widget.columnSupport(level);
    items.push(action("Remove level", "binnacle-grouping-remove-level", () => widget.removeLevel(axis, level)));
    if (other === "cols" ? support.pivot : support.group) {
      items.push(action(`Move to ${other === "cols" ? "columns" : "rows"}`, "binnacle-grouping-move-level", () => widget.moveLevel(axis, level)));
    }
    items.push(action(`Collapse to ${label(level)}`, "binnacle-grouping-collapse-to", () => widget.expandTo(axis, at)));
    // A pivot's innermost groups are its items: nothing under them to open.
    const pivot = cols.length > 0 || rows.includes(null);
    if (!(pivot && at === levels.length - 1)) {
      items.push(action(`Expand through ${label(level)}`, "binnacle-grouping-expand-through", () => widget.expandTo(axis, at + 1)));
    }
  } else if (level === null && at >= 0) {
    items.push(action(`Move to ${other === "cols" ? "columns" : "rows"}`, "binnacle-grouping-move-level", () => widget.moveLevel(axis, null)));
  }
  const valueItems: HTMLElement[] = [];
  if (values.length > 1) {
    for (const v of values) valueItems.push(action(`Remove value ${name(v)}`, "binnacle-grouping-remove-value", () => widget.removeValue(v)));
  }
  const count: AggregateValue<ColId> = { field: null, aggregate: "count" };
  const fields = widget.fields();
  valueItems.push(
    ...disclosure("Add a value", "binnacle-grouping-values", [
      ...(values.some((v) => same(v, count)) ? [] : [action(name(count), "binnacle-grouping-add-value", () => widget.addValue(count))]),
      ...fields.flatMap(addable),
    ]),
  );
  const unused = fields.filter((f) => !used(f));
  const others = disclosure(
    "Group by another column",
    "binnacle-grouping-unused",
    unused.flatMap((f) => {
      const support = widget.columnSupport(f);
      return [
        ...(support.group ? [action(`Group rows by ${label(f)}`, "binnacle-grouping-group-rows", () => widget.groupBy(f))] : []),
        ...(support.pivot
          ? [action(`Add ${label(f)} as a column level`, "binnacle-grouping-group-cols", () => widget.groupBy(f, "cols"))]
          : []),
      ];
    }),
  );
  return [items, valueItems, others].filter((section) => section.length > 0).flatMap((section, i) => (i ? [rule(), ...section] : section));
}
