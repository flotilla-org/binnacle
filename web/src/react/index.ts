// @flotilla-org/binnacle/react: a thin React wrapper around GridWidget. The
// widget owns its frame loop; React only mounts it, passes props through and
// destroys it. Plain createElement keeps the package free of JSX settings.

import { createElement, useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactElement, type Ref } from "react";
import { GridWidget, type GridOptions } from "../index";
import { live, recreates, throughLatest } from "./options";

// Everything the main entry offers, so a React app imports from one place.
export * from "../index";

export interface BinnacleGridProps<RowId, ColId, V = unknown> extends GridOptions<RowId, ColId, V> {
  /** Class of the element the grid fills; give it a size. */
  className?: string;
  style?: CSSProperties;
  /** Receives the widget once it is created, and `null` when it is destroyed; `focus`, `reveal` and `scrollTo` are called through it. */
  widgetRef?: Ref<GridWidget<RowId, ColId, V> | null>;
}

/**
 * The grid as a React component. Callbacks always see the latest props;
 * `theme` and `border` restyle the same widget; any other option is read when
 * the widget is created, so a new value (a new `source`, a new object)
 * recreates it. Memoize objects to keep the widget.
 */
export function BinnacleGrid<RowId, ColId, V = unknown>(props: BinnacleGridProps<RowId, ColId, V>): ReactElement {
  const { className, style, widgetRef, ...options } = props;
  const host = useRef<HTMLDivElement>(null);
  const latest = useRef(options);
  const widget = useRef<GridWidget<RowId, ColId, V> | null>(null);
  // The options the widget was created from, and a counter that recreates it.
  const [created, setCreated] = useState(options);
  const [generation, setGeneration] = useState(0);
  if (created !== options && recreates(created, options)) {
    setCreated(options);
    setGeneration(generation + 1);
  }

  useLayoutEffect(() => {
    latest.current = options;
  });

  useEffect(() => {
    const container = host.current;
    if (!container) return;
    let cancelled = false;
    const used = throughLatest(latest);
    void GridWidget.create(container, used).then((made) => {
      if (cancelled) {
        made.destroy();
        return;
      }
      widget.current = made;
      // Live options that changed while the wasm module loaded.
      for (const [key, value] of live(latest.current)) {
        if (value !== (used as Record<string, unknown>)[key]) Object.assign(made, { [key]: value });
      }
      assign(widgetRef, made);
    });
    return () => {
      cancelled = true;
      widget.current?.destroy();
      widget.current = null;
      assign(widgetRef, null);
    };
    // Recreated only when the generation moves; widgetRef is read at creation.
  }, [generation]);

  const { theme, border } = options;
  useEffect(() => {
    if (widget.current && theme !== undefined) widget.current.theme = theme;
  }, [theme]);
  useEffect(() => {
    if (widget.current && border !== undefined) widget.current.border = border;
  }, [border]);

  return createElement("div", { ref: host, className, style });
}

function assign<T>(ref: Ref<T | null> | undefined, value: T | null): void {
  if (typeof ref === "function") ref(value);
  else if (ref) ref.current = value;
}
