// A small table for the empty and error states, in the bundled Inter so the
// screenshots do not depend on the machine's fonts. `?rows=0` starts it
// empty. `window.fixture` filters every row out (the source then says so),
// makes the source fail its Windows and cells, retries with a Reset, and
// records what `onError` was called with.

import { GridWidget, light, type AxisSink, type FailedRequest, type TableSource } from "../../src";

const NAMES = ["Ada", "Grace", "Edsger", "Barbara", "Donald"];
const HEADERS = ["name", "crew"];

const params = new URL(location.href).searchParams;
const rows = Number(params.get("rows") ?? NAMES.length);
let filtered = false;
let failing = false;
let sink: { rows: AxisSink<string> } | null = null;
const errors: { message: string; request: FailedRequest<string, string> }[] = [];

const ids = () => (filtered ? [] : NAMES.slice(0, rows));
const fail = <T>(answer: () => T): Promise<T> =>
  failing ? Promise.reject(new Error("the server is down")) : Promise.resolve(answer());

const source: TableSource<string, string, string> = {
  rows: { count: () => ids().length, window: (s, e) => fail(() => ids().slice(s, e)) },
  cols: { count: () => HEADERS.length, window: (s, e) => HEADERS.slice(s, e) },
  cells: (rs, cs) => fail(() => rs.map((r) => cs.map((c) => (c === "name" ? r : String(r.length))))),
  emptyMessage: () => (filtered ? "No rows match the filter" : undefined),
  attach: (s) => {
    sink = s;
    return () => (sink = null);
  },
};

const theme = { ...light, font: "13px Inter", headerFont: "600 12px Inter" };
await Promise.all([document.fonts.load(theme.font), document.fonts.load(theme.headerFont)]);
const host = document.querySelector<HTMLElement>("#grid")!;
const widget = await GridWidget.create(host, {
  source,
  theme,
  label: "People",
  columnWidth: 120,
  onError: (error, request) => errors.push({ message: (error as Error).message, request }),
  onFrame: (frame) => {
    if (frame.painted) host.dataset.painted = String(Number(host.dataset.painted ?? 0) + 1);
    host.dataset.loading = String(frame.loading);
  },
});

Object.assign(window, {
  fixture: {
    widget,
    errors,
    filter(on: boolean) {
      filtered = on;
      sink?.rows.notify({ type: "Reset" });
    },
    fail(on: boolean) {
      failing = on;
    },
    retry() {
      sink?.rows.notify({ type: "Reset" });
    },
  },
});
