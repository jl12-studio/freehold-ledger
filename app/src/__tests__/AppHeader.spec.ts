import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";

const invoke = vi.fn();
const revealItemInDir = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  revealItemInDir: (...a: unknown[]) => revealItemInDir(...a),
}));

import App from "../App.vue";
import AppHeader from "../components/AppHeader.vue";
import { seedInvoke } from "./fixtures/mockInvoke";
import { businesses } from "./fixtures/seed";

const PATH = "/Users/pat/Ledger/freehold-ledger.sqlite3";

describe("App header and footer (S-UI-17)", () => {
  beforeEach(() => {
    invoke.mockReset();
    revealItemInDir.mockReset();
    revealItemInDir.mockResolvedValue(undefined);
    invoke.mockImplementation((cmd: string, args?: never) =>
      cmd === "ledger_info"
        ? Promise.resolve({ path: PATH, schema_version: 4, transaction_count: 3, current_business_id: 1 })
        : seedInvoke(cmd, args),
    );
  });

  it("s_ui_17a_header_holds_brand_switcher_export_connect_without_the_path", async () => {
    const wrapper = mount(AppHeader, {
      props: { showExport: true, businesses, selectedId: 1, currentId: 1 },
    });
    await flushPromises();

    const header = wrapper.get("header");
    expect(header.get("h1").text()).toBe("Freehold Ledger");
    const order = ["Freehold Ledger", "Business", "Tax year", "Export", "Connect your agent"];
    const text = header.text();
    let at = -1;
    for (const part of order) {
      const next = text.indexOf(part, at + 1);
      expect(next, part).toBeGreaterThan(at);
      at = next;
    }
    const connect = header.get("button.btn.btn-primary");
    expect(connect.text()).toBe("Connect your agent");
    expect(text).not.toContain("Show in Finder");
    expect(text).not.toContain("Ledger file");
    expect(header.find("[title]").exists()).toBe(false);
  });

  it("s_ui_17d_brand_mark_is_decorative", () => {
    const wrapper = mount(AppHeader, {
      props: { showExport: false, businesses, selectedId: 1, currentId: 1 },
    });
    expect(wrapper.get("h1").element.previousElementSibling?.getAttribute("aria-hidden")).toBe("true");
    expect(wrapper.get("header svg").attributes("aria-hidden")).toBe("true");
  });

  it("s_ui_17b_footer_shows_path_and_reveals_it", async () => {
    const wrapper = mount(App);
    await flushPromises();

    const footer = wrapper.get("footer");
    expect(footer.text()).toContain("Ledger file");
    expect(footer.get(`[title="${PATH}"]`).text()).toBe(PATH);
    await footer.get("button.link").trigger("click");
    expect(revealItemInDir).toHaveBeenCalledWith(PATH);
    expect(wrapper.get("header").text()).not.toContain(PATH);
  });

  it("s_ui_17b_footer_shows_in_the_empty_state", async () => {
    invoke.mockImplementation((cmd: string, args?: never) =>
      cmd === "list_businesses"
        ? Promise.resolve([{ ...businesses[0], transaction_count: 0 }])
        : cmd === "ledger_info"
          ? Promise.resolve({ path: PATH, schema_version: 4, transaction_count: 0, current_business_id: 1 })
          : seedInvoke(cmd, args),
    );
    const wrapper = mount(App);
    await flushPromises();
    expect(wrapper.get("footer").text()).toContain("Show in Finder");
    expect(wrapper.get("footer").find(`[title="${PATH}"]`).exists()).toBe(true);
  });

  it("s_ui_17b_footer_shows_in_the_error_state_with_the_errors_path", async () => {
    invoke.mockRejectedValue({ kind: "not_a_ledger", message: "x", path: PATH });
    const wrapper = mount(App);
    await flushPromises();
    const footer = wrapper.get("footer");
    expect(footer.find(`[title="${PATH}"]`).exists()).toBe(true);
    await footer.get("button.link").trigger("click");
    expect(revealItemInDir).toHaveBeenCalledWith(PATH);
    expect(wrapper.get("header").text()).not.toContain("Tax year");
  });

  it("s_ui_17c_export_control_uses_control_and_btn_and_outcome_links", async () => {
    const FOLDER = "/Users/pat/Documents/Freehold Ledger/exports/x";
    let finish!: (v: unknown) => void;
    invoke.mockImplementation((cmd: string, args?: never) =>
      cmd === "ledger_years"
        ? Promise.resolve([2026])
        : cmd === "export_tax_year"
          ? new Promise((r) => (finish = r))
          : cmd === "ledger_info"
            ? Promise.resolve({ path: PATH, schema_version: 4, transaction_count: 3, current_business_id: 1 })
            : seedInvoke(cmd, args),
    );
    const wrapper = mount(App);
    await flushPromises();

    const header = wrapper.get("header");
    expect(header.get("select#export-year").classes()).toContain("control");
    const exportBtn = header.findAll("button.btn").find((b) => b.text() === "Export")!;
    await exportBtn.trigger("click");
    const working = header.findAll("button.btn").find((b) => b.text() === "Exporting…")!;
    expect(working.attributes("aria-disabled")).toBe("true");

    finish({ folder: FOLDER, files: [] });
    await flushPromises();
    const status = header.get("[role='status']");
    expect(status.text()).toContain(`Exported My business 2026 to ${FOLDER}`);
    expect(status.findAll("button.link").map((b) => b.text())).toEqual(["Show in Finder", "Dismiss"]);
  });
});
