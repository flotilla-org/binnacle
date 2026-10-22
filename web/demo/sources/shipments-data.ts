// The shipments demo's rows, apart from the widget so end-to-end tests can
// compute what they expect from the same data.

import { PORTS } from "./records-data";

export interface Shipment {
  id: number;
  port: string;
  /** The day it shipped, at midnight UTC, through 2024. */
  shipped: Date;
  /** Missing for every ninth shipment, to show blanks never comparing. */
  weight: number | null;
  crates: number;
}

export const SHIPMENTS = 2000;

const DAY = 86_400_000;
const START = Date.UTC(2024, 0, 1);

function generate(i: number): Shipment {
  // A cheap scramble so the columns are not already in order.
  const h = Number((BigInt(i) * 0x9e3779b97f4a7c15n) % (1n << 64n) >> 40n);
  return {
    id: i,
    port: PORTS[h % PORTS.length]!,
    shipped: new Date(START + ((h >> 4) % 366) * DAY),
    weight: i % 9 === 0 ? null : ((h >> 8) % 5_000) / 10,
    crates: (h >> 12) % 40,
  };
}

export const shipments = Array.from({ length: SHIPMENTS }, (_, i) => generate(i));

/** A date as the shipped column writes it: `YYYY-MM-DD`. */
export const day = (d: Date) => d.toISOString().slice(0, 10);
