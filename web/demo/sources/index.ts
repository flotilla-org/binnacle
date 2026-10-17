// The demo's sources, one module each. Add a source by adding its module and
// listing it here.

import { huge } from "./huge";
import { records } from "./records";

import type { DemoSource } from "./demo-source";

export type { DemoSource, Mounted } from "./demo-source";

export const SOURCES: readonly DemoSource[] = [records, huge];
