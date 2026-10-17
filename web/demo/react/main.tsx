// The React page: the records source through BinnacleGrid. The status line
// is React state set from onFrame, so the page re-renders on every frame
// while the grid does not. onFrame is a new closure each render, reading the
// current Theme and Border names: the wrapper hands the latest one to the
// same widget. Theme, Border Style and the filter restyle or notify that widget;
// "Compact rows" changes a creation option, so it makes a new one. Cells edit
// in place as on the main demo; onCommit is a closure too, counting edits in
// React state. Everything comes from the /react entry, which re-exports the
// package's main entry.

import { StrictMode, useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  BinnacleGrid,
  borderName,
  borderStyles,
  byColumn,
  catppuccinMocha,
  fromRows,
  presets,
  systemTheme,
  textRenderer,
  type FrameInfo,
  type GridWidget,
  type ThemeChoice,
} from "../../src/react";
import { parse, portPicker, type Value } from "../sources/records";
import { matches, rows, type RecordRow } from "../sources/records-data";
import "./style.css";

const SYSTEM = "system";
const number = new Intl.NumberFormat("en");
const weight = textRenderer<Value>({ align: "end", format: new Intl.NumberFormat("en", { minimumFractionDigits: 1 }) });
const renderer = byColumn<keyof RecordRow, Value>((col) => (col === "weight" ? weight : undefined));

function App() {
  const source = useMemo(
    () => fromRows(rows, [{ key: "id" }, { key: "name" }, { key: "weight" }, { key: "crew" }, { key: "port" }]),
    [],
  );
  const [themeName, setThemeName] = useState(SYSTEM);
  const [borderIndex, setBorderIndex] = useState(2);
  const [filter, setFilter] = useState("");
  const [compact, setCompact] = useState(false);
  const [shown, setShown] = useState(true);
  const [line, setLine] = useState("React  ·  loading");
  const [edits, setEdits] = useState(0);
  const [committed, setCommitted] = useState("");

  const widget = useRef<GridWidget<number, keyof RecordRow, unknown> | null>(null);

  // The page's chrome follows the grid's Theme, as on the main demo.
  useEffect(() => {
    const dark = widget.current?.theme === catppuccinMocha;
    document.documentElement.dataset.scheme = dark ? "dark" : "light";
  });

  const themeLabel = () => {
    const name = widget.current?.theme.name ?? "";
    return themeName === SYSTEM ? `${name} (system)` : name;
  };
  const theme: ThemeChoice = presets.find((t) => t.name === themeName) ?? systemTheme;
  const border = borderStyles[borderIndex]!;

  const onCommit = (id: number, col: keyof RecordRow, value: Value | string) => {
    const parsed = parse(col, value);
    const edit = edits + 1;
    setEdits(edit);
    if (parsed === undefined) {
      setCommitted(`${col} of ${id} kept (edit ${edit})`);
    } else {
      (source.row(id) as Record<keyof RecordRow, Value>)[col] = parsed;
      setCommitted(`${col} of ${id} = ${JSON.stringify(parsed)} (edit ${edit})`);
    }
    source.changed(id);
  };

  const onFilter = (text: string) => {
    setFilter(text);
    source.filter(text.trim() ? (r) => matches(r, text) : null);
  };

  return (
    <>
      <header>
        <h1>binnacle</h1>
        <nav id="sources" aria-label="Page">
          <a href="/">demo</a>
          <a href="/react.html" aria-current="page">
            React
          </a>
        </nav>
        <div id="tools">
          <input
            type="search"
            placeholder="Filter name or port"
            aria-label="Filter records"
            value={filter}
            onChange={(e) => onFilter(e.target.value)}
          />
          <output id="committed">{committed}</output>
        </div>
        <div id="look">
          <label className="check">
            <input type="checkbox" checked={compact} onChange={(e) => setCompact(e.target.checked)} /> Compact rows
          </label>
          <label className="check">
            <input type="checkbox" checked={shown} onChange={(e) => setShown(e.target.checked)} /> Show the grid
          </label>
          <label htmlFor="theme">Theme</label>
          <select id="theme" value={themeName} onChange={(e) => setThemeName(e.target.value)}>
            <option value={SYSTEM}>follow the system</option>
            {presets.map((t) => (
              <option key={t.name} value={t.name}>
                {t.name}
              </option>
            ))}
          </select>
          <label htmlFor="border">Border</label>
          <select id="border" value={borderIndex} onChange={(e) => setBorderIndex(Number(e.target.value))}>
            {borderStyles.map((b, i) => (
              <option key={borderName(b)} value={i}>
                {borderName(b)}
              </option>
            ))}
          </select>
        </div>
      </header>
      <main id="grid">
        {shown && (
          <BinnacleGrid
            className="fill"
            source={source}
            label="Records"
            columnWidth={140}
            rowHeight={compact ? 20 : 24}
            theme={theme}
            border={border}
            renderer={renderer}
            onCommit={onCommit}
            editor={(_id, col) => (col === "port" ? portPicker() : null)}
            onFrame={(frame) => setLine(describe(frame, themeLabel(), borderName(border)))}
            widgetRef={widget}
          />
        )}
      </main>
      <footer id="status" role="status">
        {shown ? line : "React  ·  no grid"}
      </footer>
    </>
  );
}

function describe(frame: FrameInfo<number, keyof RecordRow>, theme: string, border: string): string {
  const { rows: r, focusIds, focus } = frame;
  const parts = [
    "React",
    `theme ${theme}`,
    `border ${border}`,
    `rows ${number.format(r.first + 1)} to ${number.format(r.last + 1)} of ${number.format(r.count)}`,
  ];
  if (focus && focusIds) parts.push(`focus row ${number.format(focus.row + 1)} (${focusIds.row} / ${focusIds.col})`);
  return parts.join("  ·  ");
}

createRoot(document.querySelector("#root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
