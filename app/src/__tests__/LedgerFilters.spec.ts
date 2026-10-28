import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { seedInvoke } from "./fixtures/mockInvoke";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ revealItemInDir: vi.fn() }));

import App from "../App.vue";

async function launch() {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}

const lastFilter = () => {
  const calls = invoke.mock.calls.filter((c) => c[0] === "list_transactions");
  return calls[calls.length - 1][1].filter;
};
const ids = (w: ReturnType<typeof mount>) =>
  w.findAll("tbody tr[id^='tx-']").map((r) => Number(r.attributes("id")!.slice(3)));
const clearButtons = (w: ReturnType<typeof mount>) =>
  w.findAll("button").filter((b) => b.text() === "Clear filters");

describe("Ledger filters", () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockImplementation(seedInvoke);
    document.body.innerHTML = "";
  });
  afterEach(() => vi.useRealTimers());

  it("filters_send_expected_filter_and_clear_resets", async () => {
    const wrapper = await launch();
    expect(lastFilter()).toEqual({ include_voided: true });
    expect(clearButtons(wrapper)).toHaveLength(0);
    expect(wrapper.findAll("label > span").map((l) => l.text())).toEqual(
      expect.arrayContaining(["From", "To", "Kind", "Category", "Source", "Show voided entries"]),
    );

    // Kind = Income: Upwork and Etsy rows only, Source enabled
    const source = wrapper.get("select[name='source']");
    expect(source.attributes("disabled")).toBeUndefined();
    await wrapper.get("select[name='kind']").setValue("income");
    await flushPromises();
    expect(lastFilter()).toEqual({ kind: "income", include_voided: true });
    expect(ids(wrapper)).toEqual([2, 3]);
    expect(wrapper.get("select[name='source']").attributes("disabled")).toBeUndefined();
    expect(wrapper.get("select[name='source']").text()).toContain("Upwork");

    await wrapper.get("select[name='source']").setValue("Etsy");
    await flushPromises();
    expect(lastFilter()).toEqual({ kind: "income", source: "Etsy", include_voided: true });

    // Kind = Expenses disables Source and drops it
    await wrapper.get("select[name='kind']").setValue("expense");
    await flushPromises();
    expect(wrapper.get("select[name='source']").attributes("disabled")).toBeDefined();
    expect(lastFilter()).toEqual({ kind: "expense", include_voided: true });

    // Category shows the Schedule C line
    expect(wrapper.get("select[name='category']").text()).toContain("Supplies (line 22)");
    await wrapper.get("select[name='category']").setValue("advertising");
    await flushPromises();
    expect(lastFilter()).toEqual({ kind: "expense", category: "advertising", include_voided: true });

    expect(clearButtons(wrapper).length).toBeGreaterThan(0);
    await clearButtons(wrapper)[0].trigger("click");
    await flushPromises();
    expect(lastFilter()).toEqual({ include_voided: true });
    expect(ids(wrapper)).toHaveLength(10);
    expect(clearButtons(wrapper)).toHaveLength(0);
  });

  it("date_range_narrows_after_250ms_debounce", async () => {
    const wrapper = await launch();
    vi.useFakeTimers();
    await wrapper.get("input[name='from']").setValue("2026-07-01");
    await wrapper.get("input[name='to']").setValue("2026-09-30");
    const before = invoke.mock.calls.length;
    await vi.advanceTimersByTimeAsync(249);
    expect(invoke.mock.calls.length).toBe(before);
    await vi.advanceTimersByTimeAsync(2);
    vi.useRealTimers();
    await flushPromises();
    expect(lastFilter()).toEqual({ from: "2026-07-01", to: "2026-09-30", include_voided: true });
    expect(ids(wrapper)).toEqual([10, 3, 4]);
  });

  it("unchecking_show_voided_hides_voided_rows_and_keeps_replacement", async () => {
    const wrapper = await launch();
    await wrapper.get("input[type='checkbox']").setValue(false);
    await flushPromises();
    expect(lastFilter()).toEqual({ include_voided: false });
    expect(ids(wrapper)).toHaveLength(8);
    expect(ids(wrapper)).not.toContain(6);
    expect(ids(wrapper)).not.toContain(7);
    expect(ids(wrapper)).toContain(8);
  });

  it("no_match_state_differs_from_empty_ledger", async () => {
    const wrapper = await launch();
    await wrapper.get("input[name='from']").setValue("2030-01-01");
    await wrapper.get("input[name='to']").setValue("2030-12-31");
    await new Promise((r) => setTimeout(r, 300));
    await flushPromises();

    expect(wrapper.text()).toContain("No entries match these filters.");
    expect(wrapper.text()).not.toContain("Your ledger is ready");
    expect(wrapper.find("table").exists()).toBe(false);
    expect(clearButtons(wrapper).length).toBeGreaterThanOrEqual(2);

    await clearButtons(wrapper).slice(-1)[0].trigger("click");
    await flushPromises();
    expect(ids(wrapper)).toHaveLength(10);
  });
});
