// Where the transposed demo's records sit. The row header band fits the field
// names beside their marks, so its width follows the platform's font: the
// records start just past the band as drawn, read from the widget's own band.

import type { Page } from "@playwright/test";

/** The demo's column width and the Gap after it. */
export const COLUMN = 112 + 1;

/** An x inside the first record's column, 4 px past its left edge, from the layer's left. */
export async function firstRecord(page: Page): Promise<number> {
  const layer = await page.locator(".binnacle-layer").boundingBox();
  const band = await page.locator(".binnacle-band-left").boundingBox();
  if (!layer || !band) throw new Error("no grid");
  return band.x + band.width - layer.x + 1 + 4;
}
