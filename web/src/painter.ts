// The painter interface: draws a Snapshot and its Content onto an element the
// widget mounts. A canvas painter is the first; a DOM one could follow.

import type { Content } from "./content";
import type { Snapshot } from "./snapshot";

export interface Painter {
  /** What the widget mounts and keeps sized to the Grid's viewport. */
  readonly element: HTMLElement;
  /** The viewport in CSS px and the device pixel ratio. */
  resize(width: number, height: number, dpr: number): void;
  /** Draws a frame. Returns whether it wants another frame (an animation in progress). */
  paint(snapshot: Snapshot, content: Content): boolean;
  destroy?(): void;
}
