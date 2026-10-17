// What the demo page needs from a source: a name and a way to mount it.

import { GridWidget, type BorderStyle, type FrameInfo, type GridOptions, type Theme, type ThemeChoice } from "../../src";

/** What the page chooses for every source: the look and where frames are reported. */
export interface Look {
  /** The grid's accessible name. */
  label: string;
  theme: ThemeChoice;
  border: BorderStyle;
  onFrame(frame: FrameInfo): void;
}

/** A mounted source, as far as the page's toolbar drives it. */
export interface Mounted {
  get theme(): Theme;
  set theme(choice: ThemeChoice);
  border: BorderStyle;
  destroy(): void;
}

export interface DemoSource {
  name: string;
  description: string;
  /** Whether the status line shows the focused cell's IDs, for sources whose IDs are not Positions. */
  showIds?: boolean;
  /** Mounts the grid; `toolbar` takes any controls the source offers. */
  mount(host: HTMLElement, toolbar: HTMLElement, look: Look): Promise<Mounted>;
}

/** A source that is just widget options. */
export function plain<RowId, ColId, V>(
  name: string,
  description: string,
  options: GridOptions<RowId, ColId, V>,
): DemoSource {
  return {
    name,
    description,
    mount: (host, _toolbar, look) => GridWidget.create(host, { ...options, ...look }),
  };
}
