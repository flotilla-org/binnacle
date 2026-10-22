// The shipments source: two thousand shipments through 2024, through
// `fromRows`, grouped by region then port as it opens, for grouping and
// comparison filters.
//
// Each region's Group Item heads its ports, and each port's its shipments,
// with a disclosure mark and its count in the row header; the region and
// port columns leave the body while the rows group by them. Each group ends
// in a Subtotal carrying the group's total weight, and the Grand Total sits
// pinned at the bottom. A column's menu (its header's funnel, or
// Alt+ArrowDown) has the grouping menu: "Group rows by this column", "Add as
// a column level" (which makes a pivot), "Add a value ▸", and "Ungroup rows"
// while grouped. A right press on a level's name in the corner opens that
// level's menu: "Move to columns" on the port level turns the table into a
// pivot of ports across, and "Move to rows" back. The column chooser
// (Mod+Alt+O, or the menu's "Choose columns…") lists the grouped columns as
// "grouped". Sorting by the grouped column orders the
// groups; any other column orders the shipments within their group. Focus
// stays on its shipment through grouping and ungrouping. Sorting by weight
// orders the groups at every level by their total weight. `&form=header`
// draws the groups in header form instead: the region and port each a
// column of the row band, each group one cell spanning its rows. A page
// opened with a `form` shows a select in the toolbar to switch forms.
//
// The weight and crates columns hold numbers and the shipped column dates,
// so their column menus offer "Equal to" through "Between" on a number or
// date field beside the text condition and the checklist: a weight over
// 100, or the shipments of one quarter. Each comparison shows as a chip in
// the filter bar.
//
// A click on a Group Item's row header selects its whole group, and Shift
// stretches the selection by whole groups. Ctrl/Cmd-C copies what is shown,
// labels first, and the toolbar says how many rows went. Enter or a double
// click on a Summary Item's cell activates it, and the toolbar shows the
// drill-down it carries: the conditions that pick out its shipments.
//
// Ctrl/Cmd-F in the grid (or Find in the toolbar) opens the find bar: it
// matches the ports' and regions' labels and the shipments' cells, counts
// the matches the closed ports hide ("3 of 12, 5 in collapsed groups"), and
// a step to one opens its port. A shipment's row header drags within its
// port; a drop in another port is refused, the drop line saying so. Weight
// and crates edit in place, and "Reroute" sends the focused shipment to the
// next port: it stays where it is, dimmed, its groups' figures change at
// once, and the next sort, filter or grouping puts it in its new port.
// Group rows and totals never edit or move.
//
// With `?pace=brisk`, `slow` or `glacial` the rows group through
// groupedSource over a slow aggregate backend instead of at once: the
// toolbar picks the pace and counts the queries in flight and the groups
// and Leaves the grouping layer holds. Groups not yet answered, and Leaves,
// draw pending (a "…" in the band and loading cells) until they arrive.

import {
  createChooser,
  createColumnMenu,
  createFilterBar,
  createFindBar,
  fromRows,
  GridWidget,
  groupedSource,
  LAYOUT_VERSION,
  rowsBy,
  textRenderer,
  type Condition,
  type GroupedAxis,
  type RowsColId,
  type TableSource,
} from "../../src";
import { AGGREGATE_PACES, slowBackend, type AggregatePace } from "./aggregates";
import type { DemoSource } from "./demo-source";
import { layoutControls } from "./layout";
import { PORTS } from "./records-data";
import { day, SHIPMENTS, shipments, type Shipment } from "./shipments-data";

/** The `form` the page asks for, if any: `&form=header` draws each level as a column of the row band. */
const askedForm = () => new URL(location.href).searchParams.get("form");
const initialForm = (): GroupedAxis["form"] => (askedForm() === "header" ? "header" : "outline");

/** A select between outline form and header form for the grouped rows. */
function formSelect(widget: GridWidget<number, RowsColId<Shipment>>): HTMLSelectElement {
  const select = document.createElement("select");
  select.setAttribute("aria-label", "Group form");
  for (const [value, text] of [
    ["outline", "Groups as an outline"],
    ["header", "Groups as level columns"],
  ] as const) {
    select.append(new Option(text, value));
  }
  select.value = initialForm();
  select.addEventListener("change", () => widget.setRowForm(select.value === "header" ? "header" : "outline"));
  return select;
}

const WIDTHS = { id: 64, region: 96, port: 128, shipped: 112, weight: 96, crates: 80 } as const;
const HEADER = 28;

/** A drill-down's condition as the toolbar shows it. */
function described({ id, kind, not }: Condition<RowsColId<Shipment>>): string {
  const what =
    kind.type === "OneOf" ? `${not ? "not " : ""}${kind.values.map((v) => v ?? "(blank)").join(" or ")}` : `${not ? "not " : ""}${JSON.stringify(kind)}`;
  return `${id} ${what}`;
}

export const shipmentsDemo: DemoSource = {
  name: "shipments",
  description: `${SHIPMENTS.toLocaleString("en")} shipments by region then port, with Subtotals and a Grand Total: group by any column, compare numbers and dates`,
  showIds: true,
  filterBar: true,
  async mount(host, toolbar, look) {
    const number = textRenderer<Shipment[keyof Shipment]>({ align: "end" });
    // A total weight to one decimal place, as each shipment's is.
    const weight = textRenderer({ align: "end", format: (w: number | null) => (w === null ? "" : String(Math.round(w * 10) / 10)) });
    const table = fromRows(shipments, [
      { key: "id", filterable: false, renderer: number },
      { key: "region" },
      { key: "port" },
      // A Summary Item's cells hold nothing but its weight.
      { key: "shipped", renderer: textRenderer({ format: (d: Date | null) => (d ? day(d) : "") }) },
      { key: "weight", renderer: weight },
      { key: "crates", renderer: number },
    ], { pivots: true });
    // Joins the toolbar only once it has something to say, leaving the page's layout as it was until then.
    const received = document.createElement("output");
    received.id = "received";
    const note = (text: string) => {
      received.textContent = text;
      if (!received.isConnected) toolbar.append(received);
    };
    const asked = new URL(location.href).searchParams.get("pace");
    let pace: AggregatePace | null = asked && asked in AGGREGATE_PACES ? (asked as AggregatePace) : null;
    const counter = document.createElement("span");
    counter.className = "requests";
    counter.setAttribute("aria-live", "off");
    const count = () => (counter.textContent = `${backend?.inFlight ?? 0} queries in flight · ${grouped?.held() ?? 0} held`);
    const backend = pace ? slowBackend(table, shipments, () => pace!, count) : undefined;
    const grouped = backend && groupedSource({ base: table, backend });
    const source: TableSource<number, RowsColId<Shipment>, unknown> = grouped ?? table;
    const speed = document.createElement("select");
    if (pace) {
      speed.setAttribute("aria-label", "How late the aggregate backend answers");
      for (const [name, { groups, leaves }] of Object.entries(AGGREGATE_PACES)) {
        speed.add(new Option(`${name}: groups ${groups / 1000} s, leaves ${leaves / 1000} s`, name));
      }
      speed.value = pace;
      speed.addEventListener("change", () => {
        pace = speed.value as AggregatePace;
        const next = new URL(location.href);
        next.searchParams.set("pace", pace);
        history.replaceState(null, "", next);
      });
      toolbar.append(speed, counter);
      count();
    }
    const frame = document.createElement("div");
    frame.className = "with-filter-bar";
    const gridBox = document.createElement("div");
    gridBox.className = "grid-box";
    frame.append(gridBox);
    host.append(frame);
    const committed = document.createElement("output");
    committed.id = "committed";
    const widget = await GridWidget.create(gridBox, {
      source,
      columnWidth: 120,
      headerHeight: HEADER,
      initialColumnWidth: (col) => (typeof col === "string" ? WIDTHS[col] : 112),
      onActivate: ({ drillDown }) => {
        if (drillDown) note(`drill-down: ${drillDown.map(described).join(", ") || "every shipment"}`);
      },
      onCopy: ({ rows }) => {
        note(`copied ${rows.length} ${rows.length === 1 ? "row" : "rows"}`);
      },
      ...look,
      onCommit(id, col, value) {
        const n = typeof value === "string" ? Number(value.trim()) : Number(value);
        const fits = (col === "weight" && Number.isFinite(n)) || (col === "crates" && Number.isInteger(n));
        if (!fits) {
          committed.textContent = `${col} of ${id} kept`;
          return false;
        }
        table.row(id)[col as "weight" | "crates"] = n;
        committed.textContent = `${col} of ${id} = ${n}`;
        table.edited(id);
        return true;
      },
    });
    let focused: number | null = null;
    const unsubscribe = widget.subscribe((frame) => {
      focused = frame.focusIds && frame.focusIds.row >= 0 ? frame.focusIds.row : null;
    });
    const reroute = document.createElement("button");
    reroute.textContent = "Reroute";
    reroute.title = "Send the focused shipment to the next port";
    reroute.addEventListener("click", () => {
      if (focused === null) return;
      const shipment = table.row(focused);
      shipment.port = PORTS[(PORTS.indexOf(shipment.port) + 1) % PORTS.length]!;
      committed.textContent = `port of ${focused} = ${shipment.port}`;
      table.edited(focused);
    });
    // As a saved layout would come back, with a row band wide enough for
    // the Subtotals' labels: the column menu's "Group rows by" waits until
    // the source says the column groups, which it does once the column is
    // in view.
    const plain = { filter: { quick: null, conditions: [] }, sort: [], hidden: [] };
    const byRegion = rowsBy<keyof Shipment>("region");
    const none = { leading: [], trailing: [] };
    widget.restoreLayout({
      version: LAYOUT_VERSION,
      pins: { rows: none, cols: none },
      sizes: { rows: [], cols: [] },
      bands: { rows: HEADER, cols: 200 },
      view: {
        rows: plain,
        cols: plain,
        grouping: {
          ...byRegion,
          rowLevels: [
            { field: "region", subtotal: "after" },
            { field: "port", subtotal: "after" },
          ],
          values: [{ field: "weight", aggregate: "sum" }, ...byRegion.values],
          // The regions open and the ports closed, so both show.
          rowAxis: { ...byRegion.rowAxis, grandTotal: "after", form: initialForm(), expand: { depth: 1, except: [] } },
        },
      },
    });
    // Only a page that asks for a form shows the select.
    const form = askedForm() === null ? null : formSelect(widget);
    if (form) toolbar.append(form);
    const layout = layoutControls(widget, "shipments");
    const columnMenu = createColumnMenu(widget);
    const chooser = createChooser(widget);
    const bar = createFilterBar(widget, { columnMenu });
    const findBar = createFindBar(widget);
    frame.prepend(bar.element, findBar.element);
    const findButton = document.createElement("button");
    findButton.textContent = "Find";
    findButton.title = "Find in the table (Ctrl/Cmd-F in the grid)";
    findButton.addEventListener("click", () => findBar.open());
    toolbar.append(findButton, reroute, committed);
    return {
      get theme() {
        return widget.theme;
      },
      set theme(choice) {
        widget.theme = choice;
      },
      get border() {
        return widget.border;
      },
      set border(style) {
        widget.border = style;
      },
      destroy() {
        unsubscribe();
        findBar.destroy();
        findButton.remove();
        reroute.remove();
        committed.remove();
        form?.remove();
        layout.destroy();
        bar.destroy();
        columnMenu.destroy();
        chooser.destroy();
        widget.destroy();
        frame.remove();
        received.remove();
        grouped?.destroy();
        table.destroy();
        speed.remove();
        counter.remove();
      },
    };
  },
};
