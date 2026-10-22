// @flotilla-org/binnacle: the binnacle Grid on a web page.

export {
  GridWidget,
  type AxisFilterInfo,
  type AxisInfo,
  type ColumnHeader,
  type ColumnInfo,
  type FindInfo,
  type FrameInfo,
  type Activation,
  type GridOptions,
  type GroupingInfo,
  type HeaderClick,
  type MenuAt,
} from "./widget";
export { createFilterBar, type FilterBar, type FilterBarOptions } from "./filterbar";
export { createColumnMenu, type ColumnMenu } from "./columnmenu";
export { createChooser, type Chooser } from "./chooser";
export { capHint, type HideOutcome } from "./choose";
export { createFindBar, type FindBar, type FindBarOptions } from "./findbar";
export type { ColumnSupport, CrossId, FindState, ItemId } from "./feed";
export type { ConditionState } from "./snapshot";
export type { Editor } from "./editing";
export type { InitialSize } from "./sizes";
export type { AxisSelection, SelectionChange } from "./selection";
export { defaultBindings, type Action, type FocusStep, type KeyBindings, type Platform } from "./keymap";
export type { PinEdge } from "./codes";
export type {
  AppliedAxisView,
  AppliedView,
  AxisSink,
  TableSink,
  AxisSource,
  AxisView,
  Aggregate,
  AggregateValue,
  ChangeNotice,
  Condition,
  Distinct,
  DistinctQuery,
  FailedRequest,
  FilterSpec,
  ListedValue,
  FindCount,
  FindMatch,
  FindMethods,
  FindQuery,
  FindStep,
  GroupedAxis,
  Grouping,
  ItemKind,
  Level,
  Located,
  MaybePromise,
  Outline,
  OutlinedWindow,
  Placement,
  Primitive,
  QuickFilterSupport,
  SetPosition,
  SortDirection,
  SortSpec,
  Support,
  TableSource,
  TextMatch,
  TextQuery,
  Totals,
  ViewSpec,
} from "./source";
export { rowsBy } from "./source";
export { LAYOUT_VERSION, type Layout, type LayoutPins } from "./layout";
export { fromRows, type RowsColumn, type RowsColumnId, type RowsGroup, type RowsOptions, type RowsSource } from "./rows";
export {
  groupedSource,
  type AggregateAnswer,
  type AggregateBackend,
  type AggregateBatch,
  type AggregateBody,
  type AggregateQuery,
  type AggregateScope,
  type Figure,
  type GroupedOptions,
  type GroupedSource,
  type GroupedSummary,
  type GroupPath,
  type GroupRow,
} from "./grouped";
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
export type { CornerLevel, CornerTier, DropTarget, PinZone, Snapshot } from "./snapshot";
export { loadBinnacle } from "./wasm";
export { COPY_LIMIT, type Copied, type CopiedRow } from "./copy";
export { transpose } from "./transpose";
