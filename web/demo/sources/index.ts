// The demo's sources, one module each. Add a source by adding its module and
// listing it here.

import type { GridOptions } from "../../src";
import { huge } from "./huge";

export interface DemoSource {
  name: string;
  description: string;
  options: GridOptions<unknown>;
}

export const SOURCES: readonly DemoSource[] = [huge];
