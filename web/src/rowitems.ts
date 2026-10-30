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
   * header draws its button only where the columns take conditions on the
   * row; a row with items alone opens by Alt+ArrowRight or `openRow`. It is
   * called whenever a row's menu is checked or opened, so keep it cheap and
   * pure.
   */
  rowItems?: (rowId: RowId) => readonly RowMenuItem[];
  /** Called with the item chosen and the row, once the menu has closed. */
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
