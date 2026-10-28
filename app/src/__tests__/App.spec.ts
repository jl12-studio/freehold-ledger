import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";

const invoke = vi.fn();
const revealItemInDir = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  revealItemInDir: (...a: unknown[]) => revealItemInDir(...a),
}));

import App from "../App.vue";
import { seedInvoke } from "./fixtures/mockInvoke";

const PATH = "/Users/pat/Ledger/freehold-ledger.sqlite3";

async function launch() {
  const wrapper = mount(App);
  await flushPromises();
  return wrapper;
}

describe("Freehold Ledger window", () => {
  beforeEach(() => {
    invoke.mockReset();
    revealItemInDir.mockReset();
    revealItemInDir.mockResolvedValue(undefined);
  });
  afterEach(() => vi.useRealTimers());

  it("s_ui_1_empty_ledger_shows_ready_state_with_path", async () => {
    invoke.mockResolvedValue({ path: PATH, schema_version: 1, transaction_count: 0 });
    const wrapper = await launch();

    expect(invoke).toHaveBeenCalledWith("ledger_info");
    expect(wrapper.text()).toContain("Your ledger is ready");
    expect(wrapper.text()).toContain("Nothing has been recorded yet.");
    expect(wrapper.text()).toContain("Show the ledger file");
    expect(wrapper.text()).toContain(PATH);
    expect(wrapper.find(`[title="${PATH}"]`).exists()).toBe(true);
  });

  it("s_ui_2_not_a_ledger_shows_error_and_path", async () => {
    const message = `${PATH} is not a Freehold ledger. Nothing was changed.`;
    invoke.mockRejectedValue({ kind: "not_a_ledger", message, path: PATH });
    const wrapper = await launch();

    expect(wrapper.text()).toContain("This ledger could not be opened");
    expect(wrapper.text()).toContain(message);
    expect(wrapper.text()).toContain(PATH);
    expect(wrapper.text()).toContain("Nothing was changed.");
    expect(wrapper.text()).not.toContain("Your ledger is ready");
    expect(wrapper.text()).toContain("Show in Finder");
  });

  it("newer_schema_error_shows_update_message", async () => {
    const message =
      "This ledger was created by a newer Freehold Ledger (schema 9); this app supports up to schema 1. Update the app to open it. Nothing was changed.";
    invoke.mockRejectedValue({ kind: "newer_schema", message, path: PATH });
    const wrapper = await launch();

    expect(wrapper.text()).toContain("This ledger could not be opened");
    expect(wrapper.text()).toContain("Update the app to open it.");
  });

  it("show_in_finder_calls_reveal_with_path", async () => {
    invoke.mockResolvedValue({ path: PATH, schema_version: 1, transaction_count: 0 });
    const wrapper = await launch();

    const header = wrapper.findAll("button").find((b) => b.text() === "Show in Finder")!;
    await header.trigger("click");
    expect(revealItemInDir).toHaveBeenCalledWith(PATH);

    revealItemInDir.mockClear();
    const action = wrapper.findAll("button").find((b) => b.text() === "Show the ledger file")!;
    await action.trigger("click");
    expect(revealItemInDir).toHaveBeenCalledWith(PATH);
  });

  it("show_in_finder_works_from_the_error_state", async () => {
    invoke.mockRejectedValue({ kind: "not_a_ledger", message: "x", path: PATH });
    const wrapper = await launch();

    const buttons = wrapper.findAll("button").filter((b) => b.text() === "Show in Finder");
    expect(buttons.length).toBeGreaterThan(0);
    await buttons[buttons.length - 1].trigger("click");
    expect(revealItemInDir).toHaveBeenCalledWith(PATH);
  });

  it("ledger_with_entries_shows_the_table_not_the_empty_state", async () => {
    invoke.mockImplementation(seedInvoke);
    const wrapper = await launch();

    expect(wrapper.find("table").exists()).toBe(true);
    expect(wrapper.text()).not.toContain("Your ledger is ready");
  });

  it("loading_caption_appears_only_after_300ms", async () => {
    vi.useFakeTimers();
    invoke.mockReturnValue(new Promise(() => {}));
    const wrapper = mount(App);

    await vi.advanceTimersByTimeAsync(299);
    expect(wrapper.text()).not.toContain("Opening your ledger…");
    await vi.advanceTimersByTimeAsync(2);
    expect(wrapper.text()).toContain("Opening your ledger…");
  });
});
