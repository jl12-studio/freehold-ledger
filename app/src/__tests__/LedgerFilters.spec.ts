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

type W = ReturnType<typeof mount>;
const lastFilter = () => {
  const calls = invoke.mock.calls.filter((c) => c[0] === "list_transactions");
  return calls[calls.length - 1][1].filter;
};
const ids = (w: W) => w.findAll("tbody tr[id^='tx-']").map((r) => Number(r.attributes("id")!.slice(3)));
const select = (w: W, name: string) => w.get(`select[aria-label="${name}"]`);
const clearAll = (w: W) => w.findAll("button").filter((b) => b.text() === "Clear all");
const count = (w: W) => w.get("form[role='search'] [aria-live='polite']").text();
const pick = async (w: W, name: string, value: string) => {
  await select(w, name).setValue(value);
  await flushPromises();
};

describe("Ledger filters", () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockImplementation(seedInvoke);
    document.body.innerHTML = "";
  });
  afterEach(() => vi.useRealTimers());

  it("s_ui_21_named_periods_send_their_date_range", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 9, 28));
    const wrapper = await launch();
    expect(lastFilter()).toEqual({ include_voided: true, business_id: 1 });

    const cases: [string, string, string][] = [
      ["this_month", "2026-10-01", "2026-10-31"],
      ["last_month", "2026-09-01", "2026-09-30"],
      ["this_quarter", "2026-10-01", "2026-12-31"],
      ["last_quarter", "2026-07-01", "2026-09-30"],
      ["this_year", "2026-01-01", "2026-12-31"],
      ["last_year", "2025-01-01", "2025-12-31"],
    ];
    for (const [value, from, to] of cases) {
      await pick(wrapper, "Dates", value);
      expect(lastFilter()).toEqual({ from, to, include_voided: true, business_id: 1 });
    }
    await pick(wrapper, "Dates", "");
    expect(lastFilter()).toEqual({ include_voided: true, business_id: 1 });
  });

  it("s_ui_21_custom_range_shows_date_inputs_and_sends_typed_dates_after_250ms", async () => {
    const wrapper = await launch();
    expect(wrapper.find("input[aria-label='From']").exists()).toBe(false);
    await pick(wrapper, "Dates", "custom");
    expect(wrapper.find("input[aria-label='From']").exists()).toBe(true);
    expect(wrapper.find("input[aria-label='To']").exists()).toBe(true);

    vi.useFakeTimers();
    await wrapper.get("input[aria-label='From']").setValue("2026-07-01");
    await wrapper.get("input[aria-label='To']").setValue("2026-09-30");
    const before = invoke.mock.calls.length;
    await vi.advanceTimersByTimeAsync(249);
    expect(invoke.mock.calls.length).toBe(before);
    await vi.advanceTimersByTimeAsync(2);
    vi.useRealTimers();
    await flushPromises();
    expect(lastFilter()).toEqual({ from: "2026-07-01", to: "2026-09-30", include_voided: true, business_id: 1 });
    expect(ids(wrapper)).toEqual([10, 3, 4]);

    await pick(wrapper, "Dates", "this_year");
    expect(wrapper.find("input[aria-label='From']").exists()).toBe(false);
    expect(wrapper.find("input[aria-label='To']").exists()).toBe(false);
  });

  it("s_ui_22_type_is_a_radio_fieldset_that_filters_and_hides_source_for_expenses", async () => {
    const wrapper = await launch();
    const fieldset = wrapper.get("fieldset");
    expect(fieldset.get("legend").text()).toBe("Type");
    const radios = fieldset.findAll("input[type='radio']");
    expect(radios).toHaveLength(3);
    expect(fieldset.findAll("label").map((l) => l.text())).toEqual(["All", "Expenses", "Income"]);
    expect(new Set(radios.map((r) => r.attributes("name"))).size).toBe(1);

    await radios[2].setValue(true);
    await flushPromises();
    expect(lastFilter()).toEqual({ kind: "income", include_voided: true, business_id: 1 });
    expect(ids(wrapper)).toEqual([2, 3]);
    await pick(wrapper, "Source", "Etsy");
    expect(lastFilter()).toEqual({ kind: "income", source: "Etsy", include_voided: true, business_id: 1 });

    await radios[1].setValue(true);
    await flushPromises();
    expect(wrapper.find("select[aria-label='Source']").exists()).toBe(false);
    expect(lastFilter()).toEqual({ kind: "expense", include_voided: true, business_id: 1 });
  });

  it("s_ui_23_voided_entries_are_hidden_by_default_and_counted", async () => {
    const wrapper = await launch();
    expect(ids(wrapper)).toHaveLength(8);
    expect(ids(wrapper)).not.toContain(6);
    expect(ids(wrapper)).not.toContain(7);
    expect(count(wrapper)).toBe("8 entries · 2 voided hidden");

    await wrapper.get("input[type='checkbox']").setValue(true);
    await flushPromises();
    expect(ids(wrapper)).toHaveLength(10);
    expect(count(wrapper)).toBe("10 entries");
    expect(invoke.mock.calls.filter((c) => c[0] === "list_transactions").every((c) => c[1].filter.include_voided === true)).toBe(true);
  });

  it("s_ui_24_clear_all_appears_when_a_filter_is_active_and_restores_defaults", async () => {
    const wrapper = await launch();
    expect(clearAll(wrapper)).toHaveLength(0);

    await pick(wrapper, "Dates", "this_year");
    await pick(wrapper, "Category", "advertising");
    expect(select(wrapper, "Dates").attributes("data-active")).toBe("true");
    expect(select(wrapper, "Category").attributes("data-active")).toBe("true");
    expect(clearAll(wrapper)).toHaveLength(1);

    await wrapper.get("input[type='checkbox']").setValue(true);
    await clearAll(wrapper)[0].trigger("click");
    await flushPromises();
    expect(lastFilter()).toEqual({ include_voided: true, business_id: 1 });
    expect(select(wrapper, "Dates").attributes("data-active")).toBeUndefined();
    expect((wrapper.get("input[type='checkbox']").element as HTMLInputElement).checked).toBe(false);
    expect(ids(wrapper)).toHaveLength(8);
    expect(clearAll(wrapper)).toHaveLength(0);
  });

  it("selects_have_accessible_names_and_no_visible_field_labels", async () => {
    const wrapper = await launch();
    for (const name of ["Dates", "Category", "Source", "Payment method"]) {
      expect(wrapper.find(`select[aria-label="${name}"]`).exists()).toBe(true);
    }
    expect(wrapper.findAll("form[role='search'] label > span")).toHaveLength(0);
    expect(select(wrapper, "Category").text()).toContain("Supplies (line 22)");
  });

  it("payment_method_filter_lists_methods_and_filters_rows", async () => {
    const wrapper = await launch();
    const sel = select(wrapper, "Payment method");
    expect(sel.findAll("option").map((o) => o.text())).toEqual([
      "Any payment method", "Business Visa (4242)", "Cash", "Checking (6789)", "Old Amex (0005)", "Not set",
    ]);
    expect(sel.get("optgroup").attributes("label")).toBe("Retired");

    await pick(wrapper, "Payment method", "1");
    expect(lastFilter()).toEqual({ payment_method: "1", include_voided: true, business_id: 1 });
    expect(ids(wrapper)).toEqual([1, 10]);

    await pick(wrapper, "Payment method", "none");
    expect(lastFilter()).toEqual({ payment_method: "none", include_voided: true, business_id: 1 });
    expect([...ids(wrapper)].sort((a, b) => a - b)).toEqual([5, 8]);
  });

  it("no_match_state_offers_clear_all", async () => {
    const wrapper = await launch();
    await pick(wrapper, "Dates", "custom");
    await wrapper.get("input[aria-label='From']").setValue("2030-01-01");
    await wrapper.get("input[aria-label='To']").setValue("2030-12-31");
    await new Promise((r) => setTimeout(r, 300));
    await flushPromises();

    expect(wrapper.text()).toContain("No entries match these filters.");
    expect(wrapper.find("table").exists()).toBe(false);
    expect(clearAll(wrapper).length).toBeGreaterThanOrEqual(2);

    await clearAll(wrapper).slice(-1)[0].trigger("click");
    await flushPromises();
    expect(ids(wrapper)).toHaveLength(8);
  });
});
