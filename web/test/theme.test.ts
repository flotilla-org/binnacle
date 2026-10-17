// Themes from CSS custom properties: `themeFromCss` reads `--binnacle-*` off
// an element's computed style, over a base Theme. Node has no CSS engine, so
// the computed style is a fake holding the properties a page would set.

import { afterEach, expect, test, vi } from "vitest";
import { catppuccinMocha, light, themeFromCss } from "../src";

/** Stands in for `getComputedStyle`, answering custom properties per element. */
function computedStyles(byElement: Map<Element, Record<string, string>>) {
  vi.stubGlobal("getComputedStyle", (el: Element) => ({
    getPropertyValue: (name: string) => byElement.get(el)?.[name] ?? "",
  }));
}

afterEach(() => {
  vi.unstubAllGlobals();
});

test("custom properties set the roles they name; the rest come from the base", () => {
  const el = {} as Element;
  computedStyles(
    new Map([
      [
        el,
        {
          "--binnacle-body-color": " #111111",
          "--binnacle-header-background": "rgb(1, 2, 3)",
          "--binnacle-border": "red",
          "--binnacle-font": "14px Inter",
          "--binnacle-focus-width": "3px",
          "--binnacle-shimmer-highlight": "gold",
        },
      ],
    ]),
  );
  const theme = themeFromCss(el);
  expect(theme.body).toEqual({ color: "#111111", background: light.body.background });
  expect(theme.header).toEqual({ color: light.header.color, background: "rgb(1, 2, 3)" });
  expect(theme.border).toBe("red");
  expect(theme.font).toBe("14px Inter");
  expect(theme.focus).toEqual({ color: light.focus.color, width: 3 });
  expect(theme.shimmerHighlight).toBe("gold");
  expect(theme.rowNumber).toEqual(light.rowNumber);
  expect(theme.headerFont).toBe(light.headerFont);
});

test("with no custom properties the Theme is the base's roles", () => {
  const el = {} as Element;
  computedStyles(new Map());
  const { name: _, ...roles } = themeFromCss(el, catppuccinMocha);
  const { name: __, ...base } = catppuccinMocha;
  expect(roles).toEqual(base);
});

test("zebra stripes turn off with `none` and on with a colour", () => {
  const off = {} as Element;
  const on = {} as Element;
  computedStyles(
    new Map([
      [off, { "--binnacle-zebra": "none" }],
      [on, { "--binnacle-zebra": "#f0f0f0" }],
    ]),
  );
  expect(themeFromCss(off, catppuccinMocha).zebra).toBeNull();
  expect(themeFromCss(on, light).zebra).toBe("#f0f0f0");
});

test("a width that is not a number keeps the base's", () => {
  const el = {} as Element;
  computedStyles(new Map([[el, { "--binnacle-guide-width": "thick" }]]));
  expect(themeFromCss(el).guide.width).toBe(light.guide.width);
});
