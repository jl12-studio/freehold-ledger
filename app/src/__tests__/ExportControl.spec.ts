import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";

const invoke = vi.fn();
const revealItemInDir = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  revealItemInDir: (...a: unknown[]) => revealItemInDir(...a),
}));

import App from "../App.vue";
import { seedInvoke } from "./fixtures/mockInvoke";

const FOLDER = "/Users/pat/Documents/Freehold Ledger/exports/Freehold Ledger 2026 export";

function withExport(exportImpl: () => Promise<unknown>) {
  invoke.mockImplementation((cmd: string, args?: never) => {
    if (cmd === "ledger_years") return Promise.resolve([2026, 2025]);
    if (cmd === "export_tax_year") return exportImpl();
    return seedInvoke(cmd, args);
  });
}

async function launch() {
  const wrapper = mount(App);
  await flushPromises();
  return wrapper;
}

describe("Export tax year control", () => {
  beforeEach(() => {
    invoke.mockReset();
    revealItemInDir.mockReset();
    revealItemInDir.mockResolvedValue(undefined);
  });

  it("s_ui_4_export_success_shows_folder_and_reveal", async () => {
    withExport(() => Promise.resolve({ folder: FOLDER, files: ["transactions.csv"] }));
    const wrapper = await launch();

    await wrapper.get("select[aria-label='Tax year'], select#export-year").setValue("2026");
    const button = wrapper.findAll("button").find((b) => b.text() === "Export")!;
    await button.trigger("click");
    await flushPromises();

    expect(invoke).toHaveBeenCalledWith("export_tax_year", { year: 2026 });
    const status = wrapper.get("[role='status']");
    expect(status.text()).toContain(`Exported 2026 to ${FOLDER}`);

    const reveal = status.findAll("button").find((b) => b.text() === "Show in Finder")!;
    await reveal.trigger("click");
    expect(revealItemInDir).toHaveBeenCalledWith(FOLDER);

    const dismiss = status.findAll("button").find((b) => b.text() === "Dismiss")!;
    await dismiss.trigger("click");
    expect(wrapper.find("[role='status']").exists()).toBe(false);
  });

  it("export_error_shows_message_and_nothing_written", async () => {
    withExport(() =>
      Promise.reject({ kind: "io", message: "Could not write to the Documents folder.", path: "" }),
    );
    const wrapper = await launch();

    const button = wrapper.findAll("button").find((b) => b.text() === "Export")!;
    await button.trigger("click");
    await flushPromises();

    const status = wrapper.get("[role='status']");
    expect(status.text()).toContain("Could not write to the Documents folder.");
    expect(status.text()).toContain("Nothing was written.");
    expect(status.text()).not.toContain("Exported");
  });

  it("export_error_from_core_says_nothing_was_written_only_once", async () => {
    withExport(() =>
      Promise.reject({
        kind: "other",
        message: "Could not write the export to /Users/pat/Documents: permission denied. Nothing was created.",
        path: "",
      }),
    );
    const wrapper = await launch();

    const button = wrapper.findAll("button").find((b) => b.text() === "Export")!;
    await button.trigger("click");
    await flushPromises();

    const text = wrapper.get("[role='status']").text();
    expect(text).toContain("permission denied. Nothing was created.");
    expect(text).not.toContain("Nothing was written.");
  });

  it("export_button_is_aria_disabled_while_working", async () => {
    let finish!: (v: unknown) => void;
    withExport(() => new Promise((r) => (finish = r)));
    const wrapper = await launch();

    const button = wrapper.findAll("button").find((b) => b.text() === "Export")!;
    await button.trigger("click");
    const working = wrapper.findAll("button").find((b) => b.text() === "Exporting…")!;
    expect(working.attributes("aria-disabled")).toBe("true");

    finish({ folder: FOLDER, files: [] });
    await flushPromises();
    expect(wrapper.findAll("button").some((b) => b.text() === "Export")).toBe(true);
  });

  it("years_list_defaults_to_newest", async () => {
    withExport(() => Promise.resolve({ folder: FOLDER, files: [] }));
    const wrapper = await launch();

    const select = wrapper.get("select#export-year");
    expect(select.findAll("option").map((o) => o.text())).toEqual(["2026", "2025"]);
    expect((select.element as HTMLSelectElement).value).toBe("2026");
    expect(wrapper.text()).toContain("Tax year");
  });

  it("shown_in_empty_state", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "ledger_years"
        ? Promise.resolve([2026])
        : Promise.resolve({ path: "/x/l.sqlite3", schema_version: 1, transaction_count: 0 }),
    );
    const wrapper = await launch();
    expect(wrapper.findAll("button").some((b) => b.text() === "Export")).toBe(true);
  });

  it("hidden_when_ledger_failed_to_open", async () => {
    invoke.mockRejectedValue({ kind: "not_a_ledger", message: "Not a ledger.", path: "/x" });
    const wrapper = await launch();

    expect(wrapper.text()).toContain("This ledger could not be opened");
    expect(wrapper.text()).not.toContain("Tax year");
    expect(wrapper.findAll("button").some((b) => b.text() === "Export")).toBe(false);
  });
});
