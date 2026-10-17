// Themes: the colours and fonts the canvas painter draws in, by role. This
// slice ships the roles it draws and one light preset; the rest of the roles
// and the other presets follow the ratatui Themes.

export interface Style {
  color: string;
  background: string;
}

export interface Theme {
  name: string;
  /** Body text; headers use `headerFont`. */
  font: string;
  headerFont: string;
  /** Column headers. */
  header: Style;
  /** Row numbers, down the left band. */
  rowNumber: Style;
  body: Style;
  /** Every other row's background, by Position; `null` leaves rows unstriped. */
  zebra: string | null;
  /** Gridlines in the Gaps and the header band edges. */
  border: string;
  /** The marker where a Key or value is not there yet. */
  loading: string;
  /** The ring around the focused cell, drawn while the widget has DOM focus. */
  focus: string;
  /** Laid over the selected cells and the headers of selected rows and columns; translucent. */
  selection: string;
  /** The mark beside a picked row's number. */
  picks: string;
  /** The column header band's border marks, which a resize drag grabs. */
  handle: string;
  /** The line a resize in progress draws across the grid. */
  guide: string;
  /** Where headers being moved would drop. */
  dropIndicator: string;
}

const SANS = 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif';

export const light: Theme = {
  name: "light",
  font: `13px ${SANS}`,
  headerFont: `600 12px ${SANS}`,
  header: { color: "#334155", background: "#f1f5f9" },
  rowNumber: { color: "#64748b", background: "#f8fafc" },
  body: { color: "#0f172a", background: "#ffffff" },
  zebra: "#fafbfc",
  border: "#e2e8f0",
  loading: "#94a3b8",
  focus: "#2563eb",
  selection: "rgb(37 99 235 / 0.12)",
  picks: "#2563eb",
  handle: "#94a3b8",
  guide: "#2563eb",
  dropIndicator: "#d97706",
};
