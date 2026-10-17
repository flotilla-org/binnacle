// A consumer's whole use of the package: the in-memory source from the main
// entry; the React wrapper, a Cell Renderer and editing from the /react
// subpath (which re-exports the main entry); typed IDs in the callbacks.

import { fromRows, type FrameInfo } from "@flotilla-org/binnacle";
import { BinnacleGrid, byColumn, textRenderer, type CellRenderer, type Editor } from "@flotilla-org/binnacle/react";
import { useMemo, useState } from "react";
import { createRoot } from "react-dom/client";

interface Ship {
  name: string;
  tonnage: number;
}

const ships: Ship[] = [
  { name: "Endurance", tonnage: 348 },
  { name: "Fram", tonnage: 402 },
  { name: "Discovery", tonnage: 1570 },
];

type Value = Ship[keyof Ship];

const tons: CellRenderer<Value> = textRenderer({ align: "end", format: new Intl.NumberFormat("en") });
const renderer = byColumn<keyof Ship, Value>((col) => (col === "tonnage" ? tons : undefined));
const noEditor = (): Editor<Value> | null => null;

function App() {
  const source = useMemo(() => fromRows(ships, [{ key: "name" }, { key: "tonnage" }]), []);
  const [status, setStatus] = useState("loading");
  const described = (f: FrameInfo<number, keyof Ship>) => `rows ${f.rows.first + 1} to ${f.rows.last + 1} of ${f.rows.count}`;
  return (
    <>
      <BinnacleGrid
        source={source}
        style={{ height: 300 }}
        renderer={renderer}
        editor={noEditor}
        onCommit={(row, col, value) => console.log(ships[row]?.name, col, value)}
        onFrame={(f) => setStatus(described(f))}
      />
      <p id="status">{status}</p>
    </>
  );
}

createRoot(document.querySelector("#root")!).render(<App />);
