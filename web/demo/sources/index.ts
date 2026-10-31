// The demo's sources, one module each. Add a source by adding its module and
// listing it here.

import { described } from "./described";
import { gaps } from "./gaps";
import { huge } from "./huge";
import { live } from "./live";
import { pivotDemo } from "./pivot";
import { records } from "./records";
import { shipmentsDemo } from "./shipments";
import { slow } from "./slow";
import { transposed } from "./transposed";
import { values } from "./values";

import type { DemoSource } from "./demo-source";

export type { DemoSource, Mounted } from "./demo-source";

export const SOURCES: readonly DemoSource[] = [records, shipmentsDemo, pivotDemo, slow, huge, live, gaps, transposed, values, described];
