// The values source: the shared fixture of Typed Values, every type plus a
// unit column and a time-role column, each drawn by `valueRenderer` through
// the shared formatter, as the ratatui values example draws it. A null
// shows a dim `∅`, an empty text nothing and an invalid value its raw text
// in the warning colour, marked in its corner. The page's clock starts at
// the fixture's own time, so the "seen" column's "just now" turns "1m ago"
// within the first minute; dates show in the fixture's zone.

import { fromRows, GridWidget, loadBinnacle, valueRenderer, type TypedValue } from "../../src";
import type { DemoSource } from "./demo-source";
import { valuesFixture } from "./values-data";

export const values: DemoSource = {
  name: "values",
  description: "every Typed Value through the shared formatter, with a unit and a time-role column",
  async mount(host, _toolbar, look) {
    await loadBinnacle();
    const fixture = valuesFixture();
    const start = performance.now();
    const clock = (frameNow: number) => ({ now: fixture.now + (frameNow - start), utcOffset: fixture.utcOffset });
    const source = fromRows(
      fixture.rows,
      fixture.columns.map((c) => ({
        key: c.name,
        label: c.unit ? `${c.name} (${c.unit})` : c.name,
        sortable: false,
        filterable: false,
        renderer: valueRenderer<TypedValue>({ ...c, clock }),
      })),
    );
    return GridWidget.create(host, { source, initialColumnWidth: () => "fit", ...look });
  },
};
