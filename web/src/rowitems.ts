// Row Menu Items: the items a host adds to a row's menu. The menu draws
// them; this module decides which show and what a choice reports.

import type { AxisName } from "./codes";

/** An item a host adds to a row's menu: choosing it reports `name` and the row. */
export interface RowMenuItem {
  readonly name: string;
  readonly label: string;
}

export interface ColumnMenuOptions<RowId = unknown> {
  /**
   * The items to add to row `rowId`'s menu, after the built-in ones. A row
   * with items has a menu even where the columns take no conditions on it
   * and the rows do not hide: its header draws the menu's button while
   * hovered, and Alt+ArrowRight and `openRow` open it. Summary Items are
   * asked too: under `fromRows` or `groupedSource` a Group Item's, a
   * Subtotal's or the Grand Total's row ID is negative and a row's is its
   * index (0 or more), so `rowId < 0` picks them out; another Table Source
   * gives its own IDs. It is called whenever a row's menu is checked or
   * opened and for each row header drawn, so keep it cheap and pure.
   */
  rowItems?: (rowId: RowId) => readonly RowMenuItem[];
  /** Called with the item chosen and the row whose menu it is (never the chosen set), once the menu has closed. */
  onRowItem?: (choice: { name: string; rowId: RowId }) => void;
}

/**
 * The items a menu lists: the host's for `rowId` where the menu filters the
 * columns by a row (`filtered` is `"cols"`) and is not in slicer mode, else
 * none.
 */
export function rowItemsFor<RowId>(
  options: ColumnMenuOptions<RowId> | undefined,
  filtered: AxisName,
  rowId: RowId,
  slicing: boolean,
): readonly RowMenuItem[] {
  if (slicing || filtered !== "cols") return [];
  return options?.rowItems?.(rowId) ?? [];
}

/** What choosing `item` on row `rowId` reports. */
export function rowChoice<RowId>(item: RowMenuItem, rowId: RowId): { name: string; rowId: RowId } {
  return { name: item.name, rowId };
}

/** Whether row `rowId`'s menu has host items, so it opens when nothing else would. */
export function hasRowItems<RowId>(options: ColumnMenuOptions<RowId> | undefined, rowId: RowId): boolean {
  return (options?.rowItems?.(rowId).length ?? 0) > 0;
}
