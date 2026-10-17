// Text measurement with a cache per font, and ellipsis fitting on top of it.
// The cache is cleared when web fonts finish loading, since widths change.

export interface Measure {
  /** The width of `text` in `font`, CSS px. */
  width(text: string, font: string): number;
  /** `text`, cut short with an ellipsis if needed to fit `max` CSS px in `font`. */
  fit(text: string, font: string, max: number): string;
  clear(): void;
}

const ELLIPSIS = "…";

export function createMeasure(ctx: CanvasRenderingContext2D): Measure {
  const caches = new Map<string, Map<string, number>>();
  const width = (text: string, font: string): number => {
    let cache = caches.get(font);
    if (!cache) caches.set(font, (cache = new Map()));
    let w = cache.get(text);
    if (w === undefined) {
      ctx.font = font;
      w = ctx.measureText(text).width;
      // Long-lived pages see endless distinct strings; start over rather than grow.
      if (cache.size > 20_000) cache.clear();
      cache.set(text, w);
    }
    return w;
  };
  return {
    width,
    fit(text, font, max) {
      if (width(text, font) <= max) return text;
      if (width(ELLIPSIS, font) > max) return "";
      // The longest prefix that fits with the ellipsis.
      let lo = 0;
      let hi = text.length;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (width(text.slice(0, mid) + ELLIPSIS, font) <= max) lo = mid;
        else hi = mid - 1;
      }
      return text.slice(0, lo) + ELLIPSIS;
    },
    clear: () => caches.clear(),
  };
}
