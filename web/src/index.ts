// @flotilla-org/binnacle: the binnacle Grid on a web page.

export { GridWidget, type FrameInfo, type GridOptions } from "./widget";
export { createFilterBar, type FilterBar, type FilterBarOptions } from "./filterbar";
export type { Editor } from "./editing";
export type { InitialSize } from "./sizes";
export type { AxisSelection, SelectionChange } from "./selection";
export { defaultBindings, type Action, type FocusStep, type KeyBindings, type Platform } from "./keymap";
export type {
  AppliedAxisView,
  AppliedView,
  AxisSink,
  AxisSource,
  AxisView,
  ChangeNotice,
  FailedRequest,
  FilterSpec,
  Located,
  MaybePromise,
  Primitive,
  QuickFilterSupport,
  SortDirection,
  SortSpec,
  Support,
  TableSource,
  TextQuery,
  Totals,
  ViewSpec,
} from "./source";
export { fromRows, type RowsColumn, type RowsColumnId, type RowsSource } from "./rows";
export {
  catppuccinMocha,
  light,
  mono,
  presets,
  systemTheme,
  themeFromCss,
  type ColorScheme,
  type Line,
  type Style,
  type Theme,
  type ThemeChoice,
} from "./theme";
export {
  borderName,
  borderStyles,
  headersBorder,
  type BorderExtent,
  type BorderStyle,
  type BorderWeight,
} from "./border";
export {
  byColumn,
  byRow,
  plainText,
  type Align,
  type CellCanvas,
  type CellMeasure,
  type CellRenderer,
  type CellStatus,
  type RendererChoice,
} from "./renderer";
export { textRenderer, type TextOptions } from "./renderers/text";
export { sparklineRenderer, type SparklineOptions } from "./renderers/sparkline";
export { barRenderer, type BarOptions } from "./renderers/bar";
export type { Content } from "./content";
export type { Painter } from "./painter";
export type { Snapshot } from "./snapshot";
export { loadBinnacle } from "./wasm";
