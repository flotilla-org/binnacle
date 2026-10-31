// The described source: a Described Table from a table server, through the
// dev server's `/tables` proxy (`demo/tables-proxy.ts`). `pnpm fake-tables`
// serves the made and recorded fixtures by default; `?server=` names
// another (`unix:~/.config/flotilla/run/flotilla.sock` for a real
// flotillad). The Table picker lists the server's tables; the rows stay
// live, every cell drawn through the shared formatter with its column's
// unit and role, Nested columns hidden and the key and state columns
// pinned. URL parameters: `table`, `server`, `namespace` (`flotilla`),
// `where` (repeatable), `encoding` (`ndjson` or `cbor`) and `token`.
//
// The status beside the picker says how the watch is going, and carries it
// as data attributes for the end-to-end tests: `data-state` (`connecting`,
// `loading`, `synced`, `resyncing`, `reconnecting` or `error`),
// `data-rows`, `data-partial`, `data-resyncs`, `data-reconnects` and
// `data-focus`, the focused row's primary-key tuple as JSON.

import {
  createChooser,
  createColumnMenu,
  createFilterBar,
  createFindBar,
  fromDescribed,
  GridWidget,
  light,
  loadBinnacle,
  type BorderStyle,
  type DescribedTable,
  type SelectionChange,
  type ThemeChoice,
  type TypedValue,
} from "../../src";
import type { DemoSource, Look } from "./demo-source";
import { errorMessage, watch, type WatchError, type WatchPhase } from "./described-watch";

const number = new Intl.NumberFormat("en");

/** The page's choices, from its URL. */
interface Choices {
  table: string | null;
  server: string | null;
  namespace: string;
  wheres: string[];
  encoding: "ndjson" | "cbor";
  token: string | null;
}

function choices(): Choices {
  const params = new URL(location.href).searchParams;
  return {
    table: params.get("table"),
    server: params.get("server"),
    namespace: params.get("namespace") || "flotilla",
    wheres: params.getAll("where"),
    encoding: params.get("encoding") === "cbor" ? "cbor" : "ndjson",
    token: params.get("token"),
  };
}

function headers(c: Choices): Record<string, string> {
  return {
    Accept: c.encoding === "cbor" ? "application/cbor-seq" : "application/x-ndjson",
    ...(c.server ? { "X-Binnacle-Server": c.server } : {}),
    ...(c.token ? { Authorization: `Bearer ${c.token}` } : {}),
  };
}

/** The tables a server lists: `kinds[].tables[]`. */
async function listing(c: Choices): Promise<string[]> {
  const response = await fetch("/tables", { headers: { ...headers(c), Accept: "application/json" } });
  if (!response.ok) throw { status: response.status, message: await errorMessage(response) } satisfies WatchError;
  const body = (await response.json()) as { kinds?: { tables?: string[] }[] };
  return (body.kinds ?? []).flatMap((k) => k.tables ?? []);
}

/** One table's grid, its controls and its watch. */
interface Shown {
  name: string;
  widget: GridWidget<bigint, string, TypedValue>;
  table: DescribedTable;
  destroy(): void;
}

export const described: DemoSource = {
  name: "described",
  description: "a described table from a table server, live: pick a table; ?server= names the server",
  filterBar: true,
  async mount(host, toolbar, look: Look) {
    await loadBinnacle();
    const c = choices();
    const picker = document.createElement("select");
    picker.setAttribute("aria-label", "Table");
    const status = document.createElement("output");
    status.id = "described-status";
    toolbar.append(picker, status);
    const frame = document.createElement("div");
    frame.className = "with-filter-bar";
    host.append(frame);

    let theme: ThemeChoice = look.theme;
    let border: BorderStyle = look.border;
    let shown: Shown | null = null;
    let destroyed = false;
    /** Bumped by each `show`: a mount that a later one overtook while it awaited tears itself down. */
    let generation = 0;

    const say = (state: string, text: string, data: Record<string, string> = {}) => {
      status.dataset.state = state;
      for (const [k, v] of Object.entries(data)) status.dataset[k] = v;
      status.textContent = text;
    };

    const show = async (name: string) => {
      const mine = ++generation;
      shown?.destroy();
      shown = null;
      const url = new URL(location.href);
      url.searchParams.set("table", name);
      history.replaceState(null, "", url);
      if (![...picker.options].some((o) => o.value === name)) picker.add(new Option(name, name));
      picker.value = name;

      const table = fromDescribed(name, { namespace: c.namespace, wheres: c.wheres });
      const gridBox = document.createElement("div");
      gridBox.className = "grid-box";
      frame.append(gridBox);
      let focus: unknown[] | undefined;
      let phase: WatchPhase = "connecting";
      let failure: WatchError | undefined;
      let everSynced = false;
      let laid = false;

      const widget = await GridWidget.create(gridBox, {
        source: table.source,
        columnWidth: 120,
        ...look,
        theme,
        border,
        label: `${name} table`,
        onSelectionChange(change: SelectionChange<bigint, string>) {
          focus = change.focus ? table.rowKey(change.focus.rowId) : undefined;
          tellStatus();
          look.onSelectionChange(change);
        },
      });
      if (mine !== generation || destroyed) {
        widget.destroy();
        table.destroy();
        gridBox.remove();
        return;
      }
      const columnMenu = createColumnMenu(widget);
      const chooser = createChooser(widget);
      const bar = createFilterBar(widget, { columnMenu });
      const findBar = createFindBar(widget);
      frame.prepend(bar.element, findBar.element);

      const state = (): string => {
        const s = table.status();
        if (phase === "error") return "error";
        if (phase === "waiting") return "reconnecting";
        if (!everSynced) return phase === "connecting" && !s.described ? "connecting" : "loading";
        return s.synced && phase === "open" ? "synced" : "resyncing";
      };
      function tellStatus() {
        const s = table.status();
        everSynced ||= s.synced;
        const parts = [name, state(), `${number.format(s.rows)} rows`];
        if (s.partial) parts.push(`${number.format(s.rows)} of more`);
        parts.push(`${number.format(s.mismatches)} mismatches`, `resyncs ${s.resyncs}`, `reconnects ${s.reconnects}`);
        if (focus) parts.push(`focus ${JSON.stringify(focus)}`);
        if (failure) parts.push(`error ${failure.status}: ${failure.message}`);
        say(state(), parts.join(" · "), {
          rows: String(s.rows),
          partial: String(s.partial),
          resyncs: String(s.resyncs),
          reconnects: String(s.reconnects),
          focus: focus ? JSON.stringify(focus) : "",
        });
      }

      const unsubscribe = table.onChange(() => {
        if (!laid && table.status().described) {
          laid = true;
          widget.restoreLayout(table.defaultLayout());
        }
        tellStatus();
      });
      const loop = watch(table, {
        headers: headers(c),
        onPhase(next, error) {
          phase = next;
          failure = error;
          tellStatus();
        },
      });
      tellStatus();
      shown = {
        name,
        widget,
        table,
        destroy() {
          loop.destroy();
          unsubscribe();
          findBar.destroy();
          bar.destroy();
          columnMenu.destroy();
          chooser.destroy();
          widget.destroy();
          table.destroy();
          bar.element.remove();
          findBar.element.remove();
          gridBox.remove();
        },
      };
    };

    picker.addEventListener("change", () => void show(picker.value));
    say("connecting", "connecting");
    try {
      const tables = await listing(c);
      for (const t of tables) picker.add(new Option(t, t));
      const first = c.table ?? tables[0];
      if (first !== undefined) await show(first);
      else say("error", "the server lists no tables");
    } catch (e) {
      const failure = e as Partial<WatchError>;
      const text = failure.status ? `error ${failure.status}: ${failure.message}` : `error: ${String(e)}`;
      say("error", text);
    }

    return {
      get theme() {
        return shown?.widget.theme ?? light;
      },
      set theme(choice) {
        theme = choice;
        if (shown) shown.widget.theme = choice;
      },
      get border() {
        return shown?.widget.border ?? border;
      },
      set border(style) {
        border = style;
        if (shown) shown.widget.border = style;
      },
      destroy() {
        destroyed = true;
        shown?.destroy();
        frame.remove();
        picker.remove();
        status.remove();
      },
    };
  },
};
