// The demo's sources, one module each. Add a source by adding its module and
// listing it here.

import { gaps } from "./gaps";
import { huge } from "./huge";
import { live } from "./live";
import { records } from "./records";
import { slow } from "./slow";

import type { DemoSource } from "./demo-source";

export type { DemoSource, Mounted } from "./demo-source";

export const SOURCES: readonly DemoSource[] = [records, slow, huge, live, gaps];
