import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { seedInvoke } from "./fixtures/mockInvoke";
import { seedTransactions } from "./fixtures/seed";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ revealItemInDir: vi.fn() }));

import App from "../App.vue";
import YearSummary from "../components/YearSummary.vue";
import { monthlyTotals, scheduleCTotals } from "../lib/ledgerMath";

function withYears(years: number[]) {
  invoke.mockImplementation((cmd: string, args?: never) =>
    cmd === "ledger_years" ? Promise.resolve(years) : seedInvoke(cmd, args),
  );
}

async function launch() {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}

const tallyRows = (w: ReturnType<typeof mount>) =>
  w.findAll(".tally-row").map((r) => r.findAll(":scope > span").map((c) => c.text().replace(/\s+/g, " ")).join(" | "));

describe("Year band and Schedule C tally", () => {
  beforeEach(() => {
    invoke.mockReset();
    document.body.innerHTML = "";
    localStorage.clear();
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 10, 25));
  });
  afterEach(() => vi.useRealTimers());

  it("net_received_and_paid_out_cover_the_tax_year_without_voided_entries", async () => {
    withYears([2026, 2025]);
    const wrapper = await launch();
    expect(invoke).toHaveBeenCalledWith("year_entries", { businessId: 1, year: 2026 });
    const band = wrapper.get("section[aria-label='Tax year 2026']");
    expect(band.text()).toContain("Net for 2026, so far");
    expect(band.get("[data-testid='net']").text()).toBe("$597.91");
    expect(band.get("[data-testid='received']").text()).toBe("$1,285.00");
    expect(band.get("[data-testid='paid-out']").text()).toBe("$687.09");
  });

  it("the_tally_adds_up_entries_by_schedule_c_line", async () => {
    withYears([2026, 2025]);
    const wrapper = await launch();
    expect(wrapper.get("#tally-title").text()).toBe("Schedule C, 2026");
    expect(tallyRows(wrapper)).toEqual([
      "Line L1 | Gross receipts or sales 2 entries | $1,285.00",
      "Line L8 | Advertising 2 entries | $44.99",
      "Line L18 | Office expense 1 entry | $42.10",
      "Line L22 | Supplies 2 entries · 1 marked as asset | $600.00",
    ]);
    expect(wrapper.get("[data-testid='tally-total']").text()).toBe("$687.09");
  });

  it("filters_change_the_table_but_not_the_year_band_or_tally", async () => {
    withYears([2026, 2025]);
    const wrapper = await launch();
    const yearCalls = () => invoke.mock.calls.filter((c) => c[0] === "year_entries").length;
    const before = yearCalls();
    await wrapper.get("select[aria-label='Dates']").setValue("last_month");
    await flushPromises();
    expect(yearCalls()).toBe(before);
    expect(wrapper.get("[data-testid='net']").text()).toBe("$597.91");
  });

  it("picking_another_tax_year_reloads_the_band_and_tally", async () => {
    withYears([2026, 2025]);
    const wrapper = await launch();
    await wrapper.get("select#export-year").setValue("2025");
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith("year_entries", { businessId: 1, year: 2025 });
    const band = wrapper.get("section[aria-label='Tax year 2025']");
    expect(band.text()).toContain("Net for 2025");
    expect(band.text()).not.toContain("so far");
    expect(band.get("[data-testid='net']").text()).toBe("-$900.00");
    expect(tallyRows(wrapper)).toEqual(["Line L20b | Rent or lease: other business property 1 entry | $900.00"]);
  });

  it("switching_business_reloads_the_band_for_that_business", async () => {
    withYears([2026]);
    const wrapper = await launch();
    await wrapper.get("#business-select").setValue("2");
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith("year_entries", { businessId: 2, year: 2026 });
    const live = seedTransactions.filter((t) => t.business_id === 2 && !t.void && t.occurred_on.startsWith("2026"));
    expect(tallyRows(wrapper)).toHaveLength(scheduleCTotals(live).length);
  });
});

describe("clicking a chart month", () => {
  beforeEach(() => {
    invoke.mockReset();
    document.body.innerHTML = "";
    localStorage.clear();
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 10, 25));
    withYears([2026, 2025]);
  });
  afterEach(() => vi.useRealTimers());

  const lastFilter = () => {
    const calls = invoke.mock.calls.filter((c) => c[0] === "list_transactions");
    return calls[calls.length - 1][1].filter;
  };
  const ids = (w: ReturnType<typeof mount>) => w.findAll("tbody tr[id^='tx-']").map((r) => r.attributes("id"));
  const col = (w: ReturnType<typeof mount>, i: number) => w.findAll(".chart-col")[i];

  it("shows_that_months_entries_and_marks_the_month", async () => {
    const wrapper = await launch();
    await col(wrapper, 9).trigger("click");
    await flushPromises();
    expect(lastFilter()).toEqual({ from: "2026-10-01", to: "2026-10-31", include_voided: true, business_id: 1 });
    expect(ids(wrapper)).toEqual(["tx-1", "tx-2"]);
    expect((wrapper.get("select[aria-label='Dates']").element as HTMLSelectElement).value).toBe("custom");
    expect(col(wrapper, 9).attributes("aria-pressed")).toBe("true");
    expect(col(wrapper, 9).classes()).toContain("is-selected");
  });

  it("clicking_the_same_month_again_goes_back_to_all_dates", async () => {
    const wrapper = await launch();
    await col(wrapper, 9).trigger("click");
    await flushPromises();
    await col(wrapper, 9).trigger("click");
    await flushPromises();
    expect(lastFilter()).toEqual({ include_voided: true, business_id: 1 });
    expect(col(wrapper, 9).attributes("aria-pressed")).toBe("false");
    expect(ids(wrapper)).toHaveLength(8);
  });

  it("keeps_the_other_filters_and_works_from_the_keyboard", async () => {
    const wrapper = await launch();
    await wrapper.findAll("input[name='kind']")[1].setValue(true);
    await flushPromises();
    await col(wrapper, 6).trigger("keydown", { key: "Enter" });
    await flushPromises();
    expect(lastFilter()).toEqual({ from: "2026-07-01", to: "2026-07-31", kind: "expense", include_voided: true, business_id: 1 });
    expect(ids(wrapper)).toEqual(["tx-4"]);
  });

  it("months_that_have_not_happened_yet_are_not_buttons", async () => {
    const wrapper = await launch();
    const before = invoke.mock.calls.length;
    expect(col(wrapper, 11).attributes("role")).toBeUndefined();
    expect(col(wrapper, 11).attributes("tabindex")).toBeUndefined();
    await col(wrapper, 11).trigger("click");
    await flushPromises();
    expect(invoke.mock.calls.length).toBe(before);
    expect(col(wrapper, 10).attributes("role")).toBe("button");
    expect(col(wrapper, 9).attributes("aria-label")).toBe("October 2026: received $1,200.00, paid out $300.00");
  });
});

describe("ledger math", () => {
  it("monthly_totals_skip_voided_and_other_years", () => {
    const months = monthlyTotals(seedTransactions.filter((t) => t.business_id === 1), 2026);
    expect(months[9]).toEqual({ in_cents: 120000, out_cents: 30000 });
    expect(months[5]).toEqual({ in_cents: 0, out_cents: 1999 });
    expect(months[11]).toEqual({ in_cents: 0, out_cents: 0 });
  });

  it("lines_sort_by_number_then_letter_with_income_first", () => {
    const base = seedTransactions[0];
    const row = (id: number, kind: "income" | "expense", line: string | null, label: string) => ({
      ...base, id, kind, schedule_c_line: line, category: label, category_label: label, is_asset: false, void: null,
    });
    const lines = scheduleCTotals([
      row(1, "expense", "22", "Supplies"),
      row(2, "expense", "20b", "Rent b"),
      row(3, "expense", null, "Needs a category"),
      row(4, "expense", "20a", "Rent a"),
      row(5, "income", "1", "Gross receipts"),
      row(6, "expense", "8", "Advertising"),
    ]);
    expect(lines.map((l) => l.line)).toEqual(["1", "8", "20a", "20b", "22", null]);
  });
});

describe("month chart", () => {
  it("dims_months_that_have_not_happened_yet_and_marks_this_month", () => {
    const rows = seedTransactions.filter((t) => t.business_id === 1);
    const w = mount(YearSummary, { props: { year: 2026, rows, today: new Date(2026, 10, 25) } });
    const cols = w.findAll(".chart-col");
    expect(cols).toHaveLength(12);
    expect(cols[11].classes()).toContain("is-future");
    expect(cols[10].classes()).not.toContain("is-future");
    expect(w.get(".chart-label.is-now").text()).toBe("Nov");
    expect(cols[9].findAll(".chart-in")).toHaveLength(1);
    expect(cols[9].findAll(".chart-out")).toHaveLength(1);
  });

  it("a_future_month_with_a_future_dated_entry_stays_live_and_clickable", async () => {
    const base = seedTransactions[0];
    const prepaid = { ...base, id: 99, occurred_on: "2026-12-15", amount_cents: 4800, description: "Prepaid hosting" };
    const w = mount(YearSummary, { props: { year: 2026, rows: [prepaid], today: new Date(2026, 10, 25) } });
    const dec = w.findAll(".chart-col")[11];
    expect(dec.classes()).not.toContain("is-future");
    expect(dec.attributes("role")).toBe("button");
    expect(dec.attributes("aria-hidden")).toBeUndefined();
    expect(dec.attributes("aria-label")).toBe("December 2026: received $0.00, paid out $48.00");
    await dec.trigger("mouseenter");
    expect(w.text()).toContain("December · received $0.00 · paid out $48.00");
    await dec.trigger("click");
    expect(w.emitted("month")).toEqual([[11]]);
    expect(w.findAll(".chart-col")[10].attributes("role")).toBe("button");
  });

  it("hovering_a_month_shows_its_totals", async () => {
    const rows = seedTransactions.filter((t) => t.business_id === 1);
    const w = mount(YearSummary, { props: { year: 2026, rows, today: new Date(2026, 10, 25) } });
    await w.findAll(".chart-col")[9].trigger("mouseenter");
    expect(w.text()).toContain("October · received $1,200.00 · paid out $300.00");
    await w.findAll(".chart-col")[11].trigger("mouseenter");
    expect(w.text()).toContain("December: nothing yet");
  });
});
