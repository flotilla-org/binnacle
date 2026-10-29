// The pivot source: the two thousand shipments through `fromRows` with
// `pivots`, region then port down the rows and quarter across the columns,
// each quarter showing the shipments' total weight and their count.
//
// Both axes come from the data. Each region's Group Item heads its ports,
// which are the rows; each quarter spans its two value columns in the column
// band's top row, and the "All" columns, one per value, sit pinned at the
// right. A cell is the figure where its port and its quarter cross, blank
// where no shipment does. Pivot columns don't move; sorting the columns by
// the Grand Total row (its row menu, or a click on its header) orders the
// quarters by their total weight. A pivot column's menu, or a right press on
// a level's name in the corner, has the grouping menu: remove or move a
// level, collapse to it or expand through it, remove or add a value, or
// group by a column no level uses. The Grand Total row's menu also has a
// comparison that keeps the quarters whose total passes it (a chip across
// the columns). A click on a value column's header sorts the ports (and the
// regions) by that column.
// Unticking "Q4 shipments" takes the Q4 shipments out of the data, and with
// them the Q4 columns: a sort on one stays, not applied (a red chip), and
// applies again when they come back.

import { createChooser, createColumnMenu, createFilterBar, fromRows, GridWidget, LAYOUT_VERSION, textRenderer, type Grouping, type RowsColId } from "../../src";
import type { DemoSource } from "./demo-source";
import { layoutControls } from "./layout";
import { shipments } from "./shipments-data";

/** A shipment with the quarter it shipped in, `Q1` to `Q4`. */
export interface QuarterShipment {
  region: string | null;
  port: string;
  quarter: string;
  weight: number | null;
}

/** The shipments, each with its quarter. */
export const quarterShipments: QuarterShipment[] = shipments.map((s) => ({
  region: s.region,
  port: s.port,
  quarter: `Q${Math.floor(s.shipped.getUTCMonth() / 3) + 1}`,
  weight: s.weight,
}));

type Col = RowsColId<QuarterShipment>;

/** Ports by region down the rows, quarters across, each by total weight and count, with a Grand Total on each axis. */
export const PIVOT: Grouping<Col, number, Col> = {
  rowLevels: [
    { field: "region", subtotal: "off" },
    { field: "port", subtotal: "off" },
  ],
  colLevels: [{ field: "quarter", subtotal: "off" }, { values: true }],
  values: [
    { field: "weight", aggregate: "sum" },
    { field: null, aggregate: "count" },
  ],
  slicers: [],
  rowAxis: { grandTotal: "after", form: "outline", expand: { depth: "all", except: [] } },
  colAxis: { grandTotal: "after", form: "header", expand: { depth: "all", except: [] } },
};

const HEADER = 28;

export const pivotDemo: DemoSource = {
  name: "pivot",
  description: "The shipments as a pivot: ports by region down, quarters across, each by total weight and count",
  showIds: true,
  async mount(host, toolbar, look) {
    // A total weight to one decimal place, as each shipment's is.
    const weight = textRenderer({ align: "end", format: (w: number | null) => (w === null ? "" : String(Math.round(w * 10) / 10)) });
    const source = fromRows(
      quarterShipments,
      [{ key: "region" }, { key: "port" }, { key: "quarter" }, { key: "weight", renderer: weight }],
      { pivots: true },
    );
    const frame = document.createElement("div");
    frame.className = "with-filter-bar";
    const gridBox = document.createElement("div");
    gridBox.className = "grid-box";
    frame.append(gridBox);
    host.append(frame);
    const widget = await GridWidget.create(gridBox, {
      source,
      columnWidth: 112,
      headerHeight: HEADER,
      ...look,
    });
    // As a default view would come: the parts it sets, the others left as they are.
    const plain = { filter: { quick: null, conditions: [] }, sort: [], hidden: [] };
    widget.restoreLayout({
      version: LAYOUT_VERSION,
      bands: { rows: HEADER, cols: 180 },
      view: { rows: plain, cols: plain, grouping: PIVOT },
    });
    const layout = layoutControls(widget, "pivot");
    const columnMenu = createColumnMenu(widget);
    const chooser = createChooser(widget);
    const bar = createFilterBar(widget, { columnMenu });
    frame.prepend(bar.element);
    const q4 = document.createElement("label");
    const q4Box = document.createElement("input");
    q4Box.type = "checkbox";
    q4Box.checked = true;
    q4Box.addEventListener("change", () => source.filter(q4Box.checked ? null : (s) => s.quarter !== "Q4"));
    q4.append(q4Box, " Q4 shipments");
    toolbar.append(q4);
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
        q4.remove();
        bar.destroy();
        layout.destroy();
        columnMenu.destroy();
        chooser.destroy();
        widget.destroy();
        source.destroy();
        frame.remove();
      },
    };
  },
};
