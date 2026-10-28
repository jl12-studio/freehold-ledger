/// <reference types="node" />
import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const SRC = join(process.cwd(), "src");
const files = (dir: string, ext: string[]): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory()
      ? e.name === "__tests__" ? [] : files(join(dir, e.name), ext)
      : ext.some((x) => e.name.endsWith(x)) ? [join(dir, e.name)] : [],
  );

describe("design system guards", () => {
  it("s_ui_16_no_em_dash_in_ui_copy", () => {
    for (const f of files(SRC, [".vue", ".ts"])) {
      expect(readFileSync(f, "utf8").includes("—"), f).toBe(false);
    }
  });

  it("s_ui_16_no_hex_colors_in_components", () => {
    for (const f of files(SRC, [".vue"])) {
      expect(/#[0-9a-fA-F]{3}(?:[0-9a-fA-F]{3})?\b/.test(readFileSync(f, "utf8")), f).toBe(false);
    }
  });

  it("s_ui_16_component_classes_exist", () => {
    const css = readFileSync(join(SRC, "styles.css"), "utf8");
    for (const c of [".label", ".btn", ".btn-primary", ".link", ".control", "select.control", ".chip", ".chip-test", ".chip-muted", "--color-test", "--color-income", "outline-offset: 3px"]) {
      expect(css.includes(c), c).toBe(true);
    }
  });
});
