// PROTOTYPE (#140), throwaway: three variants of the column presentation
// controls and the pinned edge, switchable with `?variant=A|B|C` and the
// floating bar at the bottom. The grid here is a fake drawn in DOM with
// sticky Strips, and the Table Source is a fake too: pinning, hiding and
// moves are not in the core yet. Look and feel are borrowed from the column
// menu, the checklist and the filter bar; none of their structure is.

import { catppuccinMocha, light, mono, type Theme } from "../../src/theme";

type Axis = "cols" | "rows";
type Side = "lead" | "trail";
type Key = number;
type Variant = "A" | "B" | "C";

const VARIANTS: { key: Variant; name: string }[] = [
  { key: "A", name: "Spreadsheet: flat menu, checklist, count in the status line" },
  { key: "B", name: "Panel: pin switch, grid chooser, count in the bar" },
  { key: "C", name: "Palette: command menu, grouped chooser, count in the corner" },
];

const ROW_H = 26;
const HEAD_H = 32;
const RN_W = 56;
/** The least scrolling region the pinned Strips leave: about 50px of columns, three rows. */
const MIN_SCROLL = { cols: 50, rows: 3 * ROW_H };
/** Past this many rows, hiding suggests a filter instead (#134). */
const ROW_HIDE_CAP = 5;
const LATENCY = 1200;
const EDGE = 26;

// ---------------------------------------------------------------- the fake source

const COLS = [
  "Vessel", "IMO", "Flag", "Type", "Operator", "Port", "Berth", "Arrived", "ETD", "Cargo",
  "Tonnage", "Draught", "Length", "Beam", "Built", "Class", "Agent", "Pilot", "Tugs", "Crew",
  "Fuel", "Speed", "Heading", "Lat", "Lon", "Last port", "Next port", "Inspected", "Status", "Notes",
];
const VESSELS = [
  "Aurora", "Bergen Star", "Cape Wrath", "Dagny", "Eider", "Fjordline", "Gannet", "Hafren", "Isbjørn", "Juno",
  "Kittiwake", "Lofoten", "Mære", "Nordkapp", "Orkney Maid", "Petrel", "Quayside", "Ranger", "Skarv", "Tern",
  "Ulla", "Vesterål", "Whimbrel", "Xanthe", "Ymir", "Zephyr", "Albatross", "Brage", "Corrie", "Dunlin",
  "Ellida", "Fulmar", "Grimsay", "Heimdal", "Iona", "Jarl", "Kestrel", "Lyra", "Merlin", "Njord",
];
const PORTS = ["Bergen", "Ålesund", "Tromsø", "Aberdeen", "Lerwick", "Esbjerg", "Rotterdam", "Hull"];
const NUMERIC = new Set(["IMO", "Tonnage", "Draught", "Length", "Beam", "Built", "Tugs", "Crew", "Fuel", "Speed", "Heading", "Lat", "Lon"]);

function hash(a: number, b: number): number {
  let h = (a * 374761393 + b * 668265263) | 0;
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) % 100000;
}

function value(row: Key, col: Key): string {
  const name = COLS[col]!;
  const h = hash(row + 1, col + 7);
  const pick = <T>(xs: T[]) => xs[h % xs.length]!;
  switch (name) {
    case "Vessel": return VESSELS[row]!;
    case "IMO": return String(9100000 + ((row * 7919) % 899999));
    case "Flag": return pick(["NO", "GB", "DK", "NL", "FO", "IS"]);
    case "Type": return pick(["Ferry", "Tanker", "Trawler", "Bulk", "Supply", "Ro-ro"]);
    case "Operator": return pick(["Hurtigruten", "Fjord1", "NorthLink", "DFDS", "Smyril"]);
    case "Port": case "Last port": case "Next port": return pick(PORTS);
    case "Berth": return `${String.fromCharCode(65 + (h % 6))}${h % 20}`;
    case "Arrived": case "ETD": case "Inspected": return `2026-10-${String(1 + (h % 28)).padStart(2, "0")}`;
    case "Cargo": return pick(["Fish", "Timber", "Containers", "Gravel", "Passengers", "Oil"]);
    case "Class": return pick(["DNV", "LR", "BV", "ABS"]);
    case "Agent": case "Pilot": return pick(["Hansen", "Olsen", "MacLeod", "Berg", "Fraser", "Lund"]);
    case "Status": return pick(["Alongside", "Anchored", "Departed", "Due"]);
    case "Notes": return pick(["", "Awaiting pilot", "Bunkering", "Crew change", "Hold 3 sealed"]);
    default: return NUMERIC.has(name) ? String(h % 5000) : String(h);
  }
}

interface AxisModel {
  axis: Axis;
  labels: string[];
  /** The source's own order over every Key, hidden ones included. */
  order: Key[];
  hidden: Set<Key>;
  /** The Grid's pinned Key lists, in pin order (#135). */
  lead: Key[];
  trail: Key[];
  sizes: Map<Key, number>;
  /** How the source treats a sort on this axis (#136). */
  sort: "none" | "bake" | "withdraw";
  refuseNext: boolean;
  pending: Key | null;
}

function axisModel(axis: Axis, labels: string[]): AxisModel {
  return {
    axis, labels, order: labels.map((_, i) => i), hidden: new Set(), lead: [], trail: [], sizes: new Map(),
    sort: "none", refuseNext: false, pending: null,
  };
}

const cols = axisModel("cols", COLS);
const rows = axisModel("rows", VESSELS);
// A starting layout with something in every region.
cols.lead.push(0, 1);
cols.trail.push(28);
cols.hidden.add(14).add(15).add(22);
cols.sizes.set(0, 150).set(4, 130).set(29, 190);
rows.lead.push(3);
const model = { cols, rows };
const axisOf = (a: Axis) => model[a];

let focus = { row: 0 as Key, col: 5 as Key };
const picks: Record<Axis, Set<Key>> = { cols: new Set(), rows: new Set() };
let lastAction = "Loaded";
let themeName = new URL(location.href).searchParams.get("theme") ?? "light";

const params = new URL(location.href).searchParams;
let variant: Variant = (VARIANTS.find((v) => v.key === params.get("variant"))?.key ?? "A") as Variant;

// ---------------------------------------------------------------- derived layout

const shownOrder = (m: AxisModel) => m.order.filter((k) => !m.hidden.has(k));
const position = (m: AxisModel, k: Key) => shownOrder(m).indexOf(k);
const sideOf = (m: AxisModel, k: Key): Side | null => (m.lead.includes(k) ? "lead" : m.trail.includes(k) ? "trail" : null);
const size = (m: AxisModel, k: Key) => (m.axis === "rows" ? ROW_H : (m.sizes.get(k) ?? 110));
const total = (m: AxisModel, ks: Key[]) => ks.reduce((s, k) => s + size(m, k), 0);
const noun = (m: AxisModel, n = 2) => (m.axis === "cols" ? (n === 1 ? "column" : "columns") : n === 1 ? "row" : "rows");
const sideName = (m: AxisModel, s: Side) => (m.axis === "cols" ? (s === "lead" ? "left" : "right") : s === "lead" ? "top" : "bottom");

function strips(m: AxisModel) {
  const shown = shownOrder(m);
  return {
    lead: m.lead.filter((k) => !m.hidden.has(k)),
    trail: m.trail.filter((k) => !m.hidden.has(k)),
    scroll: shown.filter((k) => !m.lead.includes(k) && !m.trail.includes(k)),
  };
}

const vp = document.querySelector<HTMLElement>("#viewport")!;

/** The body's length along an axis, inside the Header Band. */
const bodyLength = (a: Axis) => (a === "cols" ? vp.clientWidth - RN_W : vp.clientHeight - HEAD_H);

/**
 * How much of each pinned Strip is drawn: together at most the body less the
 * least scrolling region, the trailing Strip clipped first, each from its
 * inside edge. A focused item in a clipped Strip slides that Strip (#137).
 */
function allot(m: AxisModel) {
  const s = strips(m);
  const leadW = total(m, s.lead);
  const trailW = total(m, s.trail);
  const room = Math.max(0, bodyLength(m.axis) - MIN_SCROLL[m.axis]);
  const leadA = Math.min(leadW, room);
  const trailA = Math.min(trailW, room - leadA);
  const focused = m.axis === "cols" ? focus.col : focus.row;
  const slide = (list: Key[], shownLen: number, from: number) => {
    const at = list.indexOf(focused);
    if (at < 0) return from;
    const start = total(m, list.slice(0, at));
    const end = start + size(m, focused);
    if (start < from) return start;
    if (end > from + shownLen) return end - shownLen;
    return from;
  };
  return {
    leadW, trailW, leadA, trailA,
    leadOff: slide(s.lead, leadA, 0),
    trailOff: slide(s.trail, trailA, trailW - trailA),
    ...s,
  };
}

// ---------------------------------------------------------------- actions on the fake

let toastTimer = 0;
function toast(text: string, kind: "error" | "info", at?: { x: number; y: number }) {
  document.querySelectorAll(".toast").forEach((t) => t.remove());
  const t = document.createElement("div");
  t.className = `toast ${kind}`;
  t.textContent = text;
  document.body.append(t);
  const r = vp.getBoundingClientRect();
  const x = at?.x ?? r.left + 12;
  const y = at?.y ?? r.top + HEAD_H + 6;
  t.style.left = `${Math.min(x, innerWidth - t.offsetWidth - 8)}px`;
  t.style.top = `${y}px`;
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => t.remove(), 3500);
}

/** Where a message about item `k` goes: under its header (columns) or beside it (rows). */
function nearHeader(m: AxisModel, k: Key) {
  const el = vp.querySelector<HTMLElement>(`[data-axis="${m.axis}"][data-k="${k}"]`);
  const r = el?.getBoundingClientRect();
  if (!r) return undefined;
  return m.axis === "cols" ? { x: r.left, y: r.bottom + 4 } : { x: r.right + 6, y: r.top };
}

/** The items an action on `k` reaches: the picks when `k` is among several, else `k`. */
const targets = (m: AxisModel, k: Key) => (picks[m.axis].has(k) && picks[m.axis].size > 1 ? [...picks[m.axis]] : [k]);

function hide(m: AxisModel, ks: Key[]) {
  if (m.axis === "rows" && ks.length > ROW_HIDE_CAP) {
    toast(`Hiding more than ${ROW_HIDE_CAP} rows at once: use a filter instead`, "error", nearHeader(m, ks[0]!));
    return;
  }
  const shown = shownOrder(m);
  const live = shown.filter((k) => !ks.includes(k));
  if (live.length === 0) {
    toast(`At least one ${noun(m, 1)} stays shown`, "error", nearHeader(m, ks[0]!));
    return;
  }
  const f = m.axis === "cols" ? focus.col : focus.row;
  if (ks.includes(f)) {
    // The source's suggested neighbour: the next item in the old order, else the previous.
    const at = shown.indexOf(f);
    const next = shown.slice(at + 1).find((k) => !ks.includes(k)) ?? shown.slice(0, at).reverse().find((k) => !ks.includes(k))!;
    if (m.axis === "cols") focus.col = next;
    else focus.row = next;
  }
  for (const k of ks) {
    m.hidden.add(k);
    picks[m.axis].delete(k);
  }
  const dormant = ks.filter((k) => sideOf(m, k));
  lastAction = `Hid ${names(m, ks)}` + (dormant.length ? ` (pins of ${names(m, dormant)} go dormant)` : "");
}

function show(m: AxisModel, ks: Key[]) {
  for (const k of ks) m.hidden.delete(k);
  lastAction = `Showed ${names(m, ks)}`;
}

function pin(m: AxisModel, k: Key, side: Side, index?: number) {
  const was = sideOf(m, k);
  const list = m[side];
  let at = index ?? list.length;
  if (was === side && list.indexOf(k) < at) at--;
  m.lead = m.lead.filter((x) => x !== k);
  m.trail = m.trail.filter((x) => x !== k);
  m[side].splice(at, 0, k);
  lastAction = was === side ? `Reordered the ${sideName(m, side)} pins` : `Pinned ${names(m, [k])} ${sideName(m, side)}`;
}

function unpin(m: AxisModel, ks: Key[]) {
  m.lead = m.lead.filter((x) => !ks.includes(x));
  m.trail = m.trail.filter((x) => !ks.includes(x));
  lastAction = `Unpinned ${names(m, ks)}`;
}

/** Pins every Key from the start up to `k`, in drawn order, capped at what fits (#137). */
function freezeUpTo(m: AxisModel, k: Key) {
  const s = strips(m);
  const drawn = [...s.lead, ...s.scroll];
  const wanted = drawn.slice(0, drawn.indexOf(k) + 1);
  const room = Math.max(0, bodyLength(m.axis) - MIN_SCROLL[m.axis]);
  const fits: Key[] = [];
  let used = 0;
  for (const x of wanted) {
    if (used + size(m, x) > room) break;
    used += size(m, x);
    fits.push(x);
  }
  // Dormant leading pins stay in the list where they were.
  const dormant = m.lead.filter((x) => m.hidden.has(x));
  m.lead = [...fits, ...dormant];
  m.trail = m.trail.filter((x) => !fits.includes(x));
  lastAction = `Froze up to ${names(m, [k])}`;
  if (fits.length < wanted.length) {
    lastAction += `: only the first ${fits.length} of ${wanted.length} fit`;
    toast(`Froze the first ${fits.length} ${noun(m, fits.length)}: the rest would not fit`, "info", nearHeader(m, k));
  }
}

/** Asks the fake source to move `k` before `before` (null: to the end); one request at a time. */
function requestMove(m: AxisModel, k: Key, before: Key | null) {
  if (m.pending !== null) {
    toast("One move at a time: the last one is still pending", "error", nearHeader(m, k));
    return;
  }
  m.pending = k;
  lastAction = `Asked the source to move ${names(m, [k])}${before === null ? " to the end" : ` before ${names(m, [before])}`}`;
  render();
  setTimeout(() => {
    m.pending = null;
    if (m.refuseNext) {
      m.refuseNext = false;
      lastAction = `The source refused the move of ${names(m, [k])}`;
      toast(`Move refused: the source's order changed under the drop`, "error", nearHeader(m, k));
    } else {
      m.order = m.order.filter((x) => x !== k);
      const at = before === null ? m.order.length : m.order.indexOf(before);
      m.order.splice(at, 0, k);
      lastAction = `Moved ${names(m, [k])}`;
      if (m.sort === "bake") {
        m.sort = "none";
        lastAction += "; the source baked the sorted order and cleared the sort";
      }
    }
    render();
    syncHarness();
  }, LATENCY);
}

const names = (m: AxisModel, ks: Key[]) =>
  ks.length > 3 ? `${ks.length} ${noun(m)}` : ks.map((k) => `"${m.labels[k]}"`).join(", ");

// ---------------------------------------------------------------- drawing the fake grid

function applyTheme() {
  const t: Theme = themeName === "mocha" ? catppuccinMocha : themeName === "mono" ? mono : light;
  const dark = t === catppuccinMocha;
  const vars: Record<string, string> = {
    font: t.font, hfont: t.headerFont, "hdr-fg": t.header.color, "hdr-bg": t.header.background,
    "hdr-hover": t.headerHovered.background, "rn-fg": t.rowNumber.color, "rn-bg": t.rowNumber.background,
    "body-fg": t.body.color, "body-bg": t.body.background, zebra: t.zebra ?? t.body.background, focus: t.focus.color,
    sel: t.selection.background, border: t.border, handle: t.border, drop: t.dropIndicator.color, sort: t.sortMarker,
    "shimmer-a": t.shimmerBase, "shimmer-b": t.shimmerHighlight, error: t.error, loading: t.loading,
    // The pinned edge is a line in the border colour; a touch darker so it reads apart from a column rule.
    edge: dark ? "#585b70" : "#94a3b8", shadow: dark ? "rgb(0 0 0 / 0.55)" : "rgb(15 23 42 / 0.28)",
    fade: t.body.background, "rn-w": `${RN_W}px`, "head-h": `${HEAD_H}px`,
  };
  for (const [k, v] of Object.entries(vars)) document.documentElement.style.setProperty(`--${k}`, v);
}

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);

function headerCell(m: AxisModel, k: Key, extra = "") {
  const picked = picks[m.axis].has(k) ? " picked" : "";
  const pending = m.pending === k ? " pending" : "";
  const isCols = m.axis === "cols";
  const focused = (isCols ? focus.col : focus.row) === k ? " focused" : "";
  const side = sideOf(m, k);
  const pinMark = variant === "B" && side ? `<span class="pinmark" title="Pinned ${sideName(m, side)}">${pinGlyph(m, side)}</span>` : "";
  // The sort markers: rows sorted by Vessel, columns sorted by the first row (axis parity).
  const sortMark = (isCols && k === 0 && rows.sort !== "none") || (!isCols && k === rows.order[0] && cols.sort !== "none")
    ? `<span class="sort">${isCols ? "▲" : "◀"}</span>` : "";
  const btn = `<button class="hbtn" data-menu tabindex="-1" title="Column menu">${variant === "C" ? "⌄" : "⋮"}</button>`;
  if (isCols) {
    return `<div class="hcell${picked}${pending}${focused}" data-axis="cols" data-k="${k}" style="width:${size(m, k)}px;height:${HEAD_H}px">${pinMark}<span class="label">${esc(m.labels[k]!)}</span>${sortMark}${btn}</div>`;
  }
  return `<div class="rhead${picked}${pending}${focused}${extra}" data-axis="rows" data-k="${k}" style="width:${RN_W}px;height:${ROW_H}px">${sortMark}${pinMark}${btn}${position(m, k) + 1}</div>`;
}

const pinGlyph = (m: AxisModel, side: Side) => (m.axis === "cols" ? (side === "lead" ? "⇤" : "⇥") : side === "lead" ? "⤒" : "⤓");

function render() {
  applyTheme();
  const c = allot(cols);
  const r = allot(rows);
  const keepX = vp.scrollLeft;
  const keepY = vp.scrollTop;
  const shownCols = shownOrder(cols).length;

  const wrap = (side: Side, list: Key[], shownW: number, fullW: number, off: number, cell: (k: Key) => string, header: boolean) => {
    const clipped = shownW < fullW;
    const cut = clipped ? list.filter((k) => {
      const start = total(cols, list.slice(0, list.indexOf(k)));
      return start < off || start + size(cols, k) > off + shownW;
    }).length : 0;
    const badge = header && clipped ? `<span class="clipbadge" title="${cut} pinned ${noun(cols, cut)} clipped">${side === "lead" ? `${cut} ›` : `‹ ${cut}`}</span>` : "";
    return `<div class="wrap ${side}${list.length ? "" : " empty"}${clipped ? " clipped" : ""}" style="width:${shownW}px">${badge}<div class="inner" style="margin-left:${-off}px">${list.map(cell).join("")}</div></div>`;
  };
  const line = (rowKey: Key | null) => {
    const cell = (ck: Key) => {
      if (rowKey === null) return headerCell(cols, ck);
      const v = value(rowKey, ck);
      const num = NUMERIC.has(COLS[ck]!) ? " num" : "";
      const f = focus.row === rowKey && focus.col === ck ? " focus" : "";
      return `<div class="cell${num}${f}" data-r="${rowKey}" data-c="${ck}" style="width:${size(cols, ck)}px">${esc(v)}</div>`;
    };
    const head = rowKey === null ? corner(shownCols) : headerCell(rows, rowKey);
    const zebra = rowKey !== null && position(rows, rowKey) % 2 === 1 ? " zebra" : "";
    return `<div class="line${rowKey === null ? " header" : ""}${zebra}" style="height:${rowKey === null ? HEAD_H : ROW_H}px">${head}` +
      wrap("lead", c.lead, c.leadA, c.leadW, c.leadOff, cell, rowKey === null) +
      c.scroll.map(cell).join("") + `<div class="spacer"></div>` +
      wrap("trail", c.trail, c.trailA, c.trailW, c.trailOff, cell, rowKey === null) + `</div>`;
  };
  const group = (side: Side, list: Key[], shownH: number, fullH: number, off: number) =>
    `<div class="group ${side === "lead" ? "top" : "bottom"}${list.length ? "" : " empty"}${shownH < fullH ? " clipped" : ""}" style="height:${shownH}px">` +
    `<div style="margin-top:${-off}px">${list.map(line).join("")}</div></div>`;

  vp.className = variant === "C" ? "edge-heavy" : "edge-line";
  vp.innerHTML = `<div class="content">${line(null)}${group("lead", r.lead, r.leadA, r.leadW, r.leadOff)}` +
    `${r.scroll.map(line).join("")}<div class="fill"></div>${group("trail", r.trail, r.trailA, r.trailW, r.trailOff)}</div>`;
  vp.scrollLeft = keepX;
  vp.scrollTop = keepY;
  edges();
  drawBar();
  drawStatus();
  drawState();
}

/** The corner over the row numbers: the column count in variant C. */
function corner(shown: number) {
  const inner = variant === "C"
    ? `<button data-chooser="cols" title="${shown} of ${COLS.length} columns shown: choose columns">${shown}/${COLS.length}</button>`
    : "";
  return `<div class="corner" style="width:${RN_W}px;height:${HEAD_H}px">${inner}</div>`;
}

/** Marks which pinned edges have body scrolled under them, for the shadows. */
function edges() {
  vp.classList.toggle("sx", vp.scrollLeft > 0);
  vp.classList.toggle("ex", vp.scrollLeft < vp.scrollWidth - vp.clientWidth - 1);
  vp.classList.toggle("sy", vp.scrollTop > 0);
  vp.classList.toggle("ey", vp.scrollTop < vp.scrollHeight - vp.clientHeight - 1);
}
vp.addEventListener("scroll", () => {
  edges();
  drawState();
});

const countText = (m: AxisModel) => `${shownOrder(m).length} of ${m.labels.length} ${noun(m)}`;

function drawBar() {
  const bar = document.querySelector<HTMLElement>("#bar")!;
  if (variant !== "B") {
    bar.innerHTML = "";
    return;
  }
  const count = (m: AxisModel) =>
    `<button class="count" data-chooser="${m.axis}" title="Choose ${noun(m)}">${shownOrder(m).length} <span class="of">of ${m.labels.length} ${noun(m)}</span></button>`;
  bar.innerHTML = `<span class="fake-field">Filter rows…</span><span style="flex:1"></span>${count(cols)}${count(rows).replace("count", "count rows-count")}`;
  bar.querySelector<HTMLElement>(".rows-count")!.style.marginLeft = "0";
}

function drawStatus() {
  const status = document.querySelector<HTMLElement>("#status")!;
  status.textContent = variant === "A" ? `${countText(cols)} · ${countText(rows)} · focus ${VESSELS[focus.row]} / ${COLS[focus.col]}` : "";
}

function drawState() {
  const describe = (m: AxisModel) => {
    const s = allot(m);
    const list = (ks: Key[]) => `[${ks.map((k) => m.labels[k] + (sideOf(m, k) && m.hidden.has(k) ? " (dormant)" : "")).join(", ")}]`;
    const sizes = [...m.sizes].map(([k, v]) => `${m.labels[k]} ${v}`).join(", ");
    return [
      `${m.axis}: ${countText(m)}`,
      `  movable  ${m.sort === "withdraw" ? "no (sorted, source withdrew it)" : m.sort === "bake" ? "yes (sorted, a move bakes)" : "yes"}`,
      `  pending  ${m.pending === null ? "none" : m.labels[m.pending]}${m.refuseNext ? "  (next move refused)" : ""}`,
      `  lead     ${list(m.lead)}`,
      `  trail    ${list(m.trail)}`,
      `  hidden   ${list([...m.hidden])}`,
      m.axis === "cols" ? `  sizes    {${sizes}}` : "",
      `  drawn    lead ${s.leadA}/${s.leadW}px  trail ${s.trailA}/${s.trailW}px` + (s.leadA < s.leadW || s.trailA < s.trailW ? "  CLIPPED" : ""),
      `  picks    [${[...picks[m.axis]].map((k) => m.labels[k]).join(", ")}]`,
    ].filter(Boolean).join("\n");
  };
  document.querySelector("#state")!.textContent = [
    `variant  ${variant}: ${VARIANTS.find((v) => v.key === variant)!.name}`,
    `theme    ${themeName}`,
    `focus    ${VESSELS[focus.row]} / ${COLS[focus.col]}`,
    `scroll   x ${Math.round(vp.scrollLeft)}  y ${Math.round(vp.scrollTop)}`,
    "",
    describe(cols),
    "",
    describe(rows),
    "",
    `last     ${lastAction}`,
  ].join("\n");
}

// ---------------------------------------------------------------- dragging headers

interface Drop {
  kind: "pin" | "move" | "unpinMove" | "unpinReturn" | "none";
  side?: Side;
  index?: number;
  before?: Key | null;
  /** Where the drop line goes, in client px along the axis; null for none. */
  line: number | null;
  label: string;
  refused?: boolean;
  zone: "lead" | "trail" | "scroll";
}

/**
 * What dropping header `k` at client coordinate `p` along its axis would do
 * (#136): into a pinned Strip pins at that point, within one reorders the
 * pins, out of one unpins (moving too where the axis can, else returning to
 * its own Position), and within the body moves where the axis can.
 */
function dropAt(m: AxisModel, k: Key, p: number): Drop {
  const isCols = m.axis === "cols";
  const vr = vp.getBoundingClientRect();
  const start = isCols ? vr.left + RN_W : vr.top + HEAD_H;
  const end = isCols ? vr.left + vp.clientWidth : vr.top + vp.clientHeight;
  const scrolled = isCols ? vp.scrollLeft : vp.scrollTop;
  const a = allot(m);
  const leadEnd = start + a.leadA;
  const trailStart = end - a.trailA;
  const src = sideOf(m, k);
  const movable = m.sort !== "withdraw";
  // Where each item of a list starts on screen, and the slot `p` falls in.
  const slot = (list: Key[], origin: number) => {
    let x = origin;
    for (let i = 0; i < list.length; i++) {
      const w = size(m, list[i]!);
      if (p < x + w / 2) return { index: i, line: x };
      x += w;
    }
    return { index: list.length, line: x };
  };
  const zone = p < leadEnd || (a.leadA === 0 && p < start + EDGE) ? "lead"
    : p >= trailStart || (a.trailA === 0 && p > end - EDGE) ? "trail" : "scroll";
  if (zone !== "scroll") {
    const list = zone === "lead" ? a.lead : a.trail;
    const s = slot(list, zone === "lead" ? start - a.leadOff : trailStart - a.trailOff);
    const line = list.length === 0 ? (zone === "lead" ? start : end - 2) : Math.min(Math.max(s.line, start), end - 2);
    const label = src === zone ? "Reorder the pins" : `Pin ${sideName(m, zone)}`;
    return { kind: "pin", side: zone, index: s.index, line, label, zone };
  }
  const scroll = a.scroll.filter((x) => x !== k);
  const s = slot(a.scroll, start + a.leadA - scrolled);
  const clampLine = (x: number) => Math.min(Math.max(x, leadEnd), trailStart - 2);
  if (src === null) {
    if (!movable) {
      return { kind: "none", line: null, label: `${cap(noun(m))} can't move while sorted: drag to an edge to pin`, refused: true, zone };
    }
    const before = a.scroll[s.index] ?? null;
    if (before === k || a.scroll[s.index - 1] === k) return { kind: "none", line: null, label: "Its own place", zone };
    return { kind: "move", before: before, line: clampLine(s.line), label: "Move here", zone };
  }
  if (movable) {
    const before = a.scroll[s.index] ?? null;
    return { kind: "unpinMove", before, line: clampLine(s.line), label: "Unpin and move here", zone };
  }
  // An axis that can't move: the item returns to its own Position, and the line shows where that is.
  const pos = position(m, k);
  const own = scroll.filter((x) => position(m, x) < pos).length;
  const ownLine = start + a.leadA - scrolled + total(m, scroll.slice(0, own));
  const off = ownLine < leadEnd || ownLine > trailStart;
  return {
    kind: "unpinReturn", line: clampLine(ownLine), zone,
    label: `Unpin: returns to its place${off ? (ownLine < leadEnd ? " (off screen, before)" : " (off screen, after)") : ""}`,
  };
}

const cap = (s: string) => s[0]!.toUpperCase() + s.slice(1);

function applyDrop(m: AxisModel, k: Key, d: Drop) {
  if (d.kind === "pin") pin(m, k, d.side!, d.index);
  else if (d.kind === "unpinReturn") unpin(m, [k]);
  else if (d.kind === "unpinMove") {
    unpin(m, [k]);
    requestMove(m, k, d.before ?? null);
  } else if (d.kind === "move") requestMove(m, k, d.before ?? null);
  render();
}

let drag: { m: AxisModel; k: Key; x: number; y: number; active: boolean; drop: Drop | null; additive: boolean } | null = null;

function drawDrag(px: number, py: number) {
  const overlay = document.querySelector<HTMLElement>("#overlay")!;
  overlay.innerHTML = "";
  if (!drag?.active) return;
  const { m, k } = drag;
  const isCols = m.axis === "cols";
  const d = (drag.drop = dropAt(m, k, isCols ? px : py));
  document.body.classList.toggle("refused", !!d.refused);
  const vr = vp.getBoundingClientRect();
  const add = (cls: string, css: Partial<CSSStyleDeclaration>, text = "") => {
    const e = document.createElement("div");
    e.className = cls;
    Object.assign(e.style, css);
    if (text) e.innerHTML = text;
    overlay.append(e);
    return e;
  };
  // Variant B: the pinned Strips (or the empty edge gutters) light up as drop zones while dragging.
  if (variant === "B") {
    const a = allot(m);
    const start = isCols ? vr.left + RN_W : vr.top + HEAD_H;
    const end = isCols ? vr.left + vp.clientWidth : vr.top + vp.clientHeight;
    for (const side of ["lead", "trail"] as const) {
      const len = Math.max(side === "lead" ? a.leadA : a.trailA, EDGE);
      const from = side === "lead" ? start : end - len;
      const hot = d.zone === side ? " hot" : "";
      const text = `Pin ${sideName(m, side)}`;
      if (isCols) add(`zone${hot}`, { left: `${from}px`, top: `${vr.top}px`, width: `${len}px`, height: `${vp.clientHeight}px` }, len > 60 ? text : "");
      else add(`zone${hot}`, { left: `${vr.left}px`, top: `${from}px`, width: `${vp.clientWidth}px`, height: `${len}px`, paddingTop: "4px" }, text);
    }
  }
  if (d.line !== null) {
    const ret = d.kind === "unpinReturn" ? " return" : "";
    if (isCols) add(`dropline${ret}`, { left: `${d.line - 1}px`, top: `${vr.top}px`, width: "2px", height: `${vp.clientHeight}px` });
    else add(`dropline${ret} h`, { left: `${vr.left}px`, top: `${d.line - 1}px`, width: `${vp.clientWidth}px`, height: "2px" });
  }
  if (variant === "C") {
    // A ghost of the header follows the pointer, saying what a drop would do.
    add(`ghost${d.refused ? " no" : ""}`, { left: `${px + 12}px`, top: `${py + 12}px` }, `${esc(m.labels[k]!)}<small>${esc(d.label)}</small>`);
  } else if (!(variant === "B" && d.zone !== "scroll")) {
    // B's lit zones already say "Pin left" and so on; the label is for the body.
    const at = d.line !== null && isCols ? { left: `${d.line + 6}px`, top: `${vr.top + HEAD_H + 6}px` } : { left: `${px + 14}px`, top: `${py + 10}px` };
    add(`droplabel${d.refused ? " no" : ""}`, at, esc(d.label));
  }
}

vp.addEventListener("pointerdown", (e) => {
  const t = e.target as HTMLElement;
  if (e.button !== 0) return;
  const chooser = t.closest<HTMLElement>("[data-chooser]");
  if (chooser) {
    openChooser(chooser.dataset.chooser as Axis, chooser.getBoundingClientRect());
    return;
  }
  const h = t.closest<HTMLElement>("[data-axis]");
  if (h) {
    const m = axisOf(h.dataset.axis as Axis);
    const k = Number(h.dataset.k);
    if (t.closest("[data-menu]")) {
      e.preventDefault();
      openMenu(m, k, t.closest("[data-menu]")!.getBoundingClientRect());
      return;
    }
    drag = { m, k, x: e.clientX, y: e.clientY, active: false, drop: null, additive: e.metaKey || e.ctrlKey || e.shiftKey };
    vp.setPointerCapture(e.pointerId);
    return;
  }
  const cell = t.closest<HTMLElement>("[data-r]");
  if (cell) {
    focus = { row: Number(cell.dataset.r), col: Number(cell.dataset.c) };
    picks.cols.clear();
    picks.rows.clear();
    lastAction = "Focused a cell";
    render();
  }
});

vp.addEventListener("pointermove", (e) => {
  if (!drag) return;
  if (!drag.active && Math.hypot(e.clientX - drag.x, e.clientY - drag.y) > 4) {
    drag.active = true;
    document.body.classList.add("dragging");
  }
  drawDrag(e.clientX, e.clientY);
});

vp.addEventListener("pointerup", () => {
  if (!drag) return;
  const { m, k, active, drop, additive } = drag;
  drag = null;
  document.body.classList.remove("dragging", "refused");
  document.querySelector("#overlay")!.innerHTML = "";
  if (active) {
    if (drop?.refused) toast(drop.label, "error", nearHeader(m, k));
    else if (drop) applyDrop(m, k, drop);
    return;
  }
  // A press without a drag focuses the item; with Ctrl, Cmd or Shift it picks it.
  if (additive) {
    if (picks[m.axis].has(k)) picks[m.axis].delete(k);
    else picks[m.axis].add(k);
  } else {
    picks[m.axis].clear();
    picks[m.axis].add(k);
  }
  if (m.axis === "cols") focus.col = k;
  else focus.row = k;
  lastAction = `Picked ${names(m, [...picks[m.axis]])}`;
  render();
});

vp.addEventListener("contextmenu", (e) => {
  const h = (e.target as HTMLElement).closest<HTMLElement>("[data-axis]");
  if (!h) return;
  e.preventDefault();
  openMenu(axisOf(h.dataset.axis as Axis), Number(h.dataset.k), new DOMRect(e.clientX, e.clientY, 0, 0));
});

// ---------------------------------------------------------------- popovers

let popClose: (() => void) | null = null;

function popover(cls: string, anchor: DOMRect, build: (el: HTMLElement, close: () => void) => void): HTMLElement {
  popClose?.();
  const el = document.createElement("div");
  el.className = cls;
  const close = () => {
    el.remove();
    document.removeEventListener("pointerdown", outside, true);
    document.removeEventListener("keydown", key, true);
    if (popClose === close) popClose = null;
  };
  const outside = (e: PointerEvent) => {
    const t = e.target as Node;
    if (!el.contains(t) && !(t instanceof Element && t.closest(".flyout, #switcher, #harness"))) close();
  };
  const key = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    }
  };
  build(el, close);
  document.body.append(el);
  if (!cls.includes("drawer")) {
    el.style.left = `${Math.max(8, Math.min(anchor.left, innerWidth - el.offsetWidth - 8))}px`;
    el.style.top = `${Math.max(8, Math.min(anchor.bottom + 4, innerHeight - el.offsetHeight - 8))}px`;
  }
  document.addEventListener("pointerdown", outside, true);
  document.addEventListener("keydown", key, true);
  popClose = close;
  return el;
}

function h<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
}

function menuItem(text: string, run: (() => void) | null, hint = "") {
  const b = h("button", "mi");
  b.type = "button";
  b.innerHTML = `<span class="grow">${esc(text)}</span>${hint ? `<span class="hint">${esc(hint)}</span>` : ""}`;
  if (run) b.addEventListener("click", run);
  else b.disabled = true;
  return b;
}

/** Runs `f`, closes the open popover and draws again. */
const act = (f: () => void) => () => {
  f();
  popClose?.();
  render();
};

/** Whether an item's pinned Strip is clipped so the item is partly or wholly out of view. */
function clippedItem(m: AxisModel, k: Key) {
  const a = allot(m);
  const side = sideOf(m, k);
  return side === "lead" ? a.leadA < a.leadW : side === "trail" ? a.trailA < a.trailW : false;
}

function openMenu(m: AxisModel, k: Key, anchor: DOMRect) {
  if (variant === "A") menuA(m, k, anchor);
  else if (variant === "B") menuB(m, k, anchor);
  else menuC(m, k, anchor);
}

/** A: a flat menu like a spreadsheet's, with "Show hidden ▸" opening a list beside it. */
function menuA(m: AxisModel, k: Key, anchor: DOMRect) {
  const ks = targets(m, k);
  const side = sideOf(m, k);
  const many = ks.length > 1;
  popover("pop", anchor, (el) => {
    el.style.width = "240px";
    const title = h("div", "title");
    title.innerHTML = many ? `<b>${ks.length} ${noun(m)}</b> picked` : `${cap(noun(m, 1))} <b>${esc(m.labels[k]!)}</b>`;
    el.append(title);
    el.append(menuItem(many ? `Hide ${ks.length} ${noun(m)}` : "Hide", act(() => hide(m, ks))));
    const hidden = [...m.hidden];
    const showHidden = menuItem("Show hidden", hidden.length ? () => flyout(m, showHidden) : null, hidden.length ? `${hidden.length} ▸` : "none");
    showHidden.addEventListener("pointerenter", () => hidden.length && flyout(m, showHidden));
    el.append(showHidden, h("div", "sep"));
    if (side !== "lead") el.append(menuItem(`Pin ${sideName(m, "lead")}`, act(() => ks.forEach((x) => pin(m, x, "lead")))));
    if (side !== "trail") el.append(menuItem(`Pin ${sideName(m, "trail")}`, act(() => ks.forEach((x) => pin(m, x, "trail")))));
    if (side) el.append(menuItem("Unpin", act(() => unpin(m, ks)), clippedItem(m, k) ? "clipped" : ""));
    if (side !== "trail" && !many) el.append(menuItem("Freeze up to here", act(() => freezeUpTo(m, k))));
    el.append(h("div", "sep"), menuItem(`Choose ${noun(m)}…`, () => openChooser(m.axis, anchor)));
  });
}

function flyout(m: AxisModel, from: HTMLElement) {
  document.querySelectorAll(".flyout").forEach((f) => f.remove());
  const r = from.getBoundingClientRect();
  const el = h("div", "pop flyout");
  el.style.width = "220px";
  const search = h("input", "field");
  search.placeholder = `Search hidden ${noun(m)}`;
  const list = h("div", "list");
  const fill = () => {
    list.textContent = "";
    const q = search.value.toLowerCase();
    const hidden = m.order.filter((k) => m.hidden.has(k) && m.labels[k]!.toLowerCase().includes(q));
    if (!q) list.append(menuItem(`Show all ${m.hidden.size}`, act(() => show(m, [...m.hidden]))));
    for (const k of hidden) {
      const side = sideOf(m, k);
      list.append(menuItem(m.labels[k]!, act(() => show(m, [k])), side ? `pinned ${sideName(m, side)}` : ""));
    }
  };
  search.addEventListener("input", fill);
  fill();
  el.append(search, list);
  document.body.append(el);
  el.style.left = `${Math.min(r.right + 4, innerWidth - 228)}px`;
  el.style.top = `${Math.min(r.top - 6, innerHeight - el.offsetHeight - 8)}px`;
  const prev = popClose;
  popClose = () => {
    el.remove();
    prev?.();
  };
}

/** B: a panel with a pin switch, two buttons and the hidden items as chips. */
function menuB(m: AxisModel, k: Key, anchor: DOMRect) {
  const ks = targets(m, k);
  const side = sideOf(m, k);
  popover("pop", anchor, (el) => {
    el.style.width = "280px";
    const title = h("div", "title");
    title.innerHTML = ks.length > 1 ? `<b>${ks.length} ${noun(m)}</b> picked` : `${cap(noun(m, 1))} <b>${esc(m.labels[k]!)}</b>`;
    const pinRow = h("div", "row");
    pinRow.append(h("span", "lbl", "Pin"));
    const seg = h("div", "seg");
    for (const [s, text] of [["lead", cap(sideName(m, "lead"))], [null, "None"], ["trail", cap(sideName(m, "trail"))]] as const) {
      const b = h("button", "", text);
      b.setAttribute("aria-pressed", String(side === s));
      b.addEventListener("click", act(() => (s ? ks.forEach((x) => pin(m, x, s)) : unpin(m, ks))));
      seg.append(b);
    }
    pinRow.append(seg);
    if (side && clippedItem(m, k)) pinRow.append(h("span", "tag", "clipped"));
    const buttons = h("div", "row");
    buttons.append(h("span", "lbl", ""));
    const freeze = h("button", "btn small", "Freeze up to here");
    freeze.disabled = side === "trail" || ks.length > 1;
    freeze.addEventListener("click", act(() => freezeUpTo(m, k)));
    const hideB = h("button", "btn small", ks.length > 1 ? `Hide ${ks.length}` : "Hide");
    hideB.addEventListener("click", act(() => hide(m, ks)));
    buttons.append(freeze, hideB);
    el.append(title, pinRow, buttons, h("div", "sep"));
    const hidden = m.order.filter((x) => m.hidden.has(x));
    el.append(h("div", "section", hidden.length ? `Hidden ${noun(m)} (${hidden.length})` : `No hidden ${noun(m)}`));
    const chips = h("div", "chips");
    for (const x of hidden.slice(0, 12)) {
      const c = h("button", "chip", `${m.labels[x]} +`);
      c.title = `Show ${m.labels[x]}`;
      c.addEventListener("click", act(() => show(m, [x])));
      chips.append(c);
    }
    if (hidden.length > 12) chips.append(h("span", "note", `and ${hidden.length - 12} more`));
    el.append(chips);
    const foot = h("div", "foot");
    if (hidden.length) {
      const all = h("button", "btn small", "Show all");
      all.addEventListener("click", act(() => show(m, hidden)));
      foot.append(all);
    }
    foot.append(h("span", "grow"));
    const choose = h("button", "btn small", `Choose ${noun(m)}…`);
    choose.style.marginLeft = "auto";
    choose.addEventListener("click", () => openChooser(m.axis, anchor));
    foot.append(choose);
    el.append(foot);
  });
}

/** C: a command palette: type to narrow the commands, Showing a hidden item is just another command. */
function menuC(m: AxisModel, k: Key, anchor: DOMRect) {
  const ks = targets(m, k);
  const side = sideOf(m, k);
  const what = ks.length > 1 ? `${ks.length} ${noun(m)}` : m.labels[k]!;
  const commands: [string, () => void, string][] = [];
  commands.push([`Hide ${what}`, () => hide(m, ks), ""]);
  if (side !== "lead") commands.push([`Pin ${what} ${sideName(m, "lead")}`, () => ks.forEach((x) => pin(m, x, "lead")), ""]);
  if (side !== "trail") commands.push([`Pin ${what} ${sideName(m, "trail")}`, () => ks.forEach((x) => pin(m, x, "trail")), ""]);
  if (side) commands.push([`Unpin ${what}`, () => unpin(m, ks), clippedItem(m, k) ? "clipped" : ""]);
  if (side !== "trail" && ks.length === 1) commands.push([`Freeze up to ${what}`, () => freezeUpTo(m, k), ""]);
  if (m.hidden.size) commands.push([`Show all hidden ${noun(m)}`, () => show(m, [...m.hidden]), String(m.hidden.size)]);
  for (const x of m.order.filter((x) => m.hidden.has(x))) commands.push([`Show ${m.labels[x]}`, () => show(m, [x]), "hidden"]);
  popover("pop", anchor, (el, close) => {
    el.style.width = "300px";
    const input = h("input", "field");
    input.placeholder = `${what}: hide, pin, show…`;
    const list = h("div");
    list.style.cssText = "max-height: 280px; overflow: auto; margin-top: 6px;";
    let sel = 0;
    let shown: typeof commands = [];
    const fill = () => {
      const q = input.value.toLowerCase().split(/\s+/).filter(Boolean);
      shown = commands.filter(([t]) => q.every((w) => t.toLowerCase().includes(w)));
      shown.push([`Choose ${noun(m)}…`, () => openChooser(m.axis, anchor), ""]);
      sel = Math.min(sel, shown.length - 1);
      list.textContent = "";
      shown.forEach(([t, run, hint], i) => {
        const b = menuItem(t, () => {
          close();
          run();
          render();
        }, hint);
        if (i === sel) b.classList.add("sel");
        list.append(b);
      });
    };
    input.addEventListener("input", () => {
      sel = 0;
      fill();
    });
    input.addEventListener("keydown", (e) => {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        sel = (sel + (e.key === "ArrowDown" ? 1 : shown.length - 1)) % shown.length;
        fill();
      } else if (e.key === "Enter") {
        e.preventDefault();
        (list.children[sel] as HTMLButtonElement | undefined)?.click();
      }
    });
    fill();
    el.append(input, list);
    queueMicrotask(() => input.focus());
  });
}

// ---------------------------------------------------------------- choosers

function openChooser(a: Axis, anchor: DOMRect) {
  const m = axisOf(a);
  if (variant === "A") chooserChecklist(m, anchor);
  else if (variant === "B") chooserGrid(m);
  else chooserGrouped(m, anchor);
}

/** A: the find and filter checklist, over the axis's items: a tick shows, an untick hides. */
function chooserChecklist(m: AxisModel, anchor: DOMRect) {
  popover("pop", anchor, (el) => {
    el.style.width = "280px";
    const title = h("div", "title");
    title.innerHTML = `Choose <b>${noun(m)}</b>`;
    const search = h("input", "field");
    search.placeholder = `Search ${noun(m)}`;
    const list = h("div", "list");
    const note = h("div", "note");
    const fill = () => {
      list.textContent = "";
      const q = search.value.toLowerCase();
      const ks = m.order.filter((k) => m.labels[k]!.toLowerCase().includes(q));
      const all = ks.every((k) => !m.hidden.has(k));
      const head = checkRow(q ? `(Show all matching "${q}")` : "(Show all)", all, "", (on) => (on ? show(m, ks) : hide(m, ks)));
      head.style.borderBottom = "1px solid var(--border)";
      list.append(head);
      for (const k of ks) {
        const side = sideOf(m, k);
        const tag = side ? `${sideName(m, side)}${m.hidden.has(k) ? ", dormant" : ""}` : "";
        list.append(checkRow(m.labels[k]!, !m.hidden.has(k), tag, (on) => (on ? show(m, [k]) : hide(m, [k]))));
      }
      note.textContent = `${countText(m)} shown`;
    };
    const checkRow = (text: string, on: boolean, tag: string, change: (on: boolean) => void) => {
      const row = h("label", "check");
      const box = h("input");
      box.type = "checkbox";
      box.checked = on;
      box.addEventListener("change", () => {
        change(box.checked);
        render();
        fill();
      });
      row.append(box, h("span", "grow", text));
      if (tag) row.append(h("span", `tag${tag.includes("dormant") ? " dormant" : ""}`, `pinned ${tag}`));
      return row;
    };
    search.addEventListener("input", fill);
    fill();
    el.append(title, search, list, note);
    queueMicrotask(() => search.focus());
  });
}

/** B: a drawer holding a small grid of the axis's items: name, shown, pinned, width; its headers sort it. */
function chooserGrid(m: AxisModel) {
  let by: "order" | "name" | "shown" | "pinned" | "width" = "order";
  popover("drawer", new DOMRect(), (el, close) => {
    const head = h("header");
    head.append(h("b", "", `${cap(noun(m))}`));
    const search = h("input", "field");
    search.placeholder = "Search";
    search.style.width = "160px";
    const x = h("button", "btn small", "Close");
    x.addEventListener("click", close);
    head.append(search, x);
    const box = h("div", "tablebox");
    const foot = h("footer");
    const fill = () => {
      const q = search.value.toLowerCase();
      const pinRank = (k: Key) => (sideOf(m, k) === "lead" ? 0 : sideOf(m, k) === "trail" ? 2 : 1);
      let ks = m.order.filter((k) => m.labels[k]!.toLowerCase().includes(q));
      const cmp: Record<typeof by, (a: Key, b: Key) => number> = {
        order: () => 0,
        name: (a, b) => m.labels[a]!.localeCompare(m.labels[b]!),
        shown: (a, b) => Number(m.hidden.has(a)) - Number(m.hidden.has(b)),
        pinned: (a, b) => pinRank(a) - pinRank(b),
        width: (a, b) => size(m, b) - size(m, a),
      };
      ks = [...ks].sort(cmp[by]);
      const table = h("table");
      const tr = h("tr");
      for (const [key, text] of [["order", "#"], ["name", "Name"], ["shown", "Shown"], ["pinned", "Pinned"], ["width", m.axis === "cols" ? "Width" : "Height"]] as const) {
        const th = h("th", "", text + (by === key && key !== "order" ? " ▲" : ""));
        th.addEventListener("click", () => {
          by = key;
          fill();
        });
        tr.append(th);
      }
      table.append(tr);
      for (const k of ks) {
        const row = h("tr", m.hidden.has(k) ? "hidden" : "");
        const pos = position(m, k);
        row.append(h("td", "note", pos < 0 ? "" : String(pos + 1)));
        row.append(h("td", "name", m.labels[k]!));
        const shownTd = h("td");
        const cb = h("input");
        cb.type = "checkbox";
        cb.checked = !m.hidden.has(k);
        cb.addEventListener("change", () => {
          if (cb.checked) show(m, [k]);
          else hide(m, [k]);
          render();
          fill();
        });
        shownTd.append(cb);
        row.append(shownTd);
        const pinTd = h("td");
        const seg = h("div", "seg");
        const side = sideOf(m, k);
        for (const [s, text] of [["lead", m.axis === "cols" ? "L" : "T"], [null, "–"], ["trail", m.axis === "cols" ? "R" : "B"]] as const) {
          const b = h("button", "", text);
          b.title = s ? `Pin ${sideName(m, s)}` : "Unpin";
          b.setAttribute("aria-pressed", String(side === s));
          if (!s) b.dataset.none = "";
          b.addEventListener("click", () => {
            if (s) pin(m, k, s);
            else unpin(m, [k]);
            render();
            fill();
          });
          seg.append(b);
        }
        pinTd.append(seg);
        if (side && m.hidden.has(k)) pinTd.append(" ", h("span", "tag dormant", "dormant"));
        row.append(pinTd);
        const wTd = h("td");
        if (m.axis === "cols") {
          const w = h("input");
          w.type = "number";
          w.step = "10";
          w.value = m.sizes.has(k) ? String(m.sizes.get(k)) : "";
          w.placeholder = "110";
          w.title = "A Size Override; blank for the default. Kept while hidden.";
          w.addEventListener("change", () => {
            const n = Number(w.value);
            if (w.value === "" || !(n > 20)) m.sizes.delete(k);
            else m.sizes.set(k, n);
            lastAction = `Set the width of ${names(m, [k])}`;
            render();
          });
          wTd.append(w);
        } else wTd.append(h("span", "note", "1 line"));
        row.append(wTd);
        table.append(row);
      }
      box.textContent = "";
      box.append(table);
      foot.textContent = `${countText(m)} shown · ${m.lead.length + m.trail.length} pinned`;
    };
    search.addEventListener("input", fill);
    fill();
    el.append(head, box, foot);
  });
}

/** C: the items grouped by where they are drawn, each with the actions that move it between groups. */
function chooserGrouped(m: AxisModel, anchor: DOMRect) {
  popover("pop", anchor, (el) => {
    el.style.width = "320px";
    const search = h("input", "field");
    search.placeholder = `Search ${noun(m)}`;
    const body = h("div");
    body.style.cssText = "max-height: 420px; overflow: auto;";
    const fill = () => {
      body.textContent = "";
      const q = search.value.toLowerCase();
      const match = (k: Key) => m.labels[k]!.toLowerCase().includes(q);
      const s = strips(m);
      const groups: [string, Key[]][] = [
        [`Pinned ${sideName(m, "lead")}`, m.lead.filter(match)],
        ["Scrolling", s.scroll.filter(match)],
        [`Pinned ${sideName(m, "trail")}`, m.trail.filter(match)],
        ["Hidden", m.order.filter((k) => m.hidden.has(k) && !sideOf(m, k) && match(k))],
      ];
      for (const [name, ks] of groups) {
        body.append(h("div", "section", `${name} (${ks.length})`));
        for (const k of ks) {
          const row = h("div", "check");
          const label = h("span", "grow", m.labels[k]!);
          if (m.hidden.has(k)) label.style.color = "var(--loading)";
          row.append(label);
          if (m.hidden.has(k) && sideOf(m, k)) row.append(h("span", "tag dormant", "dormant"));
          const btn = (text: string, title: string, f: () => void) => {
            const b = h("button", "btn small", text);
            b.title = title;
            b.addEventListener("click", () => {
              f();
              render();
              fill();
            });
            row.append(b);
          };
          const side = sideOf(m, k);
          if (m.hidden.has(k)) btn("Show", "Show", () => show(m, [k]));
          if (side) {
            const list = m[side];
            const i = list.indexOf(k);
            if (i > 0) btn("↑", "Earlier in pin order", () => pin(m, k, side, i - 1));
            btn("Unpin", "Unpin", () => unpin(m, [k]));
          } else if (!m.hidden.has(k)) {
            btn(pinGlyph(m, "lead"), `Pin ${sideName(m, "lead")}`, () => pin(m, k, "lead"));
            btn(pinGlyph(m, "trail"), `Pin ${sideName(m, "trail")}`, () => pin(m, k, "trail"));
          }
          if (!m.hidden.has(k)) btn("Hide", "Hide", () => hide(m, [k]));
          body.append(row);
        }
      }
    };
    search.addEventListener("input", fill);
    fill();
    const note = h("div", "note", `${countText(m)} shown`);
    el.append(search, body, note);
    queueMicrotask(() => search.focus());
  });
}

// ---------------------------------------------------------------- harness and switcher

function syncHarness() {
  const harness = document.querySelector<HTMLElement>("#harness")!;
  const sortSelect = (m: AxisModel) =>
    `<select data-sort="${m.axis}">` +
    [["none", "unsorted (movable)"], ["bake", "sorted, a move bakes it"], ["withdraw", "sorted, not movable"]]
      .map(([v, t]) => `<option value="${v}"${m.sort === v ? " selected" : ""}>${t}</option>`).join("") + `</select>`;
  harness.innerHTML = `<b>PROTOTYPE #140</b>` +
    `<label>theme <select id="theme">${["light", "mocha", "mono"].map((t) => `<option${t === themeName ? " selected" : ""}>${t}</option>`).join("")}</select></label>` +
    `<label>columns ${sortSelect(cols)}</label>` +
    `<label><input type="checkbox" data-refuse="cols"${cols.refuseNext ? " checked" : ""}/> refuse next column move</label>` +
    `<label>rows ${sortSelect(rows)}</label>` +
    `<label><input type="checkbox" data-refuse="rows"${rows.refuseNext ? " checked" : ""}/> refuse next row move</label>` +
    `<button id="pinmany">pin 6 more left (clip)</button><button id="reset">reset</button>`;
  harness.querySelector<HTMLSelectElement>("#theme")!.onchange = (e) => {
    themeName = (e.target as HTMLSelectElement).value;
    url();
    render();
  };
  harness.querySelectorAll<HTMLSelectElement>("[data-sort]").forEach((s) => {
    s.onchange = () => {
      axisOf(s.dataset.sort as Axis).sort = s.value as AxisModel["sort"];
      lastAction = `Source ${s.dataset.sort} sort: ${s.value}`;
      render();
    };
  });
  harness.querySelectorAll<HTMLInputElement>("[data-refuse]").forEach((c) => {
    c.onchange = () => {
      axisOf(c.dataset.refuse as Axis).refuseNext = c.checked;
      drawState();
    };
  });
  harness.querySelector<HTMLButtonElement>("#pinmany")!.onclick = () => {
    for (const k of strips(cols).scroll.slice(0, 6)) pin(cols, k, "lead");
    for (const k of strips(cols).scroll.slice(-2)) pin(cols, k, "trail");
    lastAction = "Pinned 6 more left and 2 more right, past what fits";
    render();
  };
  harness.querySelector<HTMLButtonElement>("#reset")!.onclick = () => location.reload();
}

function url() {
  const u = new URL(location.href);
  u.searchParams.set("variant", variant);
  if (themeName !== "light") u.searchParams.set("theme", themeName);
  else u.searchParams.delete("theme");
  history.replaceState(null, "", u);
}

function drawSwitcher() {
  const sw = document.querySelector<HTMLElement>("#switcher")!;
  if (!import.meta.env.DEV) {
    sw.remove();
    return;
  }
  const v = VARIANTS.find((x) => x.key === variant)!;
  sw.innerHTML = `<button data-step="-1" title="Previous variant (←)">‹</button><span>${v.key}: ${esc(v.name)}</span><button data-step="1" title="Next variant (→)">›</button>`;
}

function step(by: number) {
  const i = VARIANTS.findIndex((x) => x.key === variant);
  variant = VARIANTS[(i + by + VARIANTS.length) % VARIANTS.length]!.key;
  popClose?.();
  lastAction = `Switched to variant ${variant}`;
  url();
  drawSwitcher();
  render();
}

document.querySelector("#switcher")!.addEventListener("click", (e) => {
  const b = (e.target as HTMLElement).closest<HTMLElement>("[data-step]");
  if (b) step(Number(b.dataset.step));
});
document.addEventListener("keydown", (e) => {
  const t = e.target as HTMLElement;
  if (t.closest("input, textarea, select, [contenteditable]") || popClose) return;
  if (e.key === "ArrowLeft") step(-1);
  if (e.key === "ArrowRight") step(1);
});
document.addEventListener("pointerdown", (e) => {
  const c = (e.target as HTMLElement).closest<HTMLElement>("#bar [data-chooser]");
  if (c) openChooser(c.dataset.chooser as Axis, c.getBoundingClientRect());
});
let seen = "";
new ResizeObserver(() => {
  // Only a new size draws again: drawing can flip the scrollbars, which would loop.
  const now = `${vp.offsetWidth}x${vp.offsetHeight}`;
  if (now !== seen) {
    seen = now;
    render();
  }
}).observe(vp);

// For the screenshot script: drives a drag's feedback, or focus, without a pointer.
Object.assign(window, {
  prototypeFocus(row: Key, col: Key) {
    focus = { row, col };
    render();
  },
  prototypeDrag(axis: Axis, k: Key, x: number, y: number) {
    drag = { m: axisOf(axis), k, x: 0, y: 0, active: true, drop: null, additive: false };
    document.body.classList.add("dragging");
    drawDrag(x, y);
  },
});

url();
syncHarness();
drawSwitcher();
render();
