import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { categories, seedTransactions } from "./fixtures/seed";
import { seedInvoke } from "./fixtures/mockInvoke";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ revealItemInDir: vi.fn() }));

import App from "../App.vue";
import LedgerTable from "../components/LedgerTable.vue";

async function launch() {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}

function rowFor(wrapper: ReturnType<typeof mount>, id: number) {
  return wrapper.get(`#tx-${id}`);
}

describe("Ledger table", () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockImplementation(seedInvoke);
    document.body.innerHTML = "";
  });

  it("six_columns_with_split_paid_out_and_received", async () => {
    const wrapper = await launch();
    await wrapper.get("input[type='checkbox']").setValue(true);
    await flushPromises();
    expect(wrapper.get("table caption").text()).toBe("Ledger entries");
    expect(wrapper.findAll("th[scope='col']").map((h) => h.text())).toEqual([
      "Day", "Description", "Category", "Paid with", "Paid out", "Received",
    ]);
    const rows = wrapper.findAll("tbody tr[id^='tx-']");
    expect(rows).toHaveLength(10);
    expect(rows[0].text()).toContain("Laptop for client work");
    expect(rows[9].text()).toContain("Booth rent, winter market");
    for (const r of rows) expect(r.findAll("td")).toHaveLength(6);
    expect(wrapper.get("table").classes()).toEqual(expect.arrayContaining(["table-fixed", "min-w-[44rem]"]));
  });

  it("entries_are_grouped_by_month_with_month_totals", async () => {
    const wrapper = await launch();
    const groups = wrapper.findAll("tbody[data-month]");
    const heading = (g: (typeof groups)[number]) => {
      const name = g.get("th[scope='rowgroup'] span.font-serif");
      return `${name.element.firstChild!.textContent!.trim()} ${name.get("span").text()}`;
    };
    expect(groups.map(heading)).toEqual([
      "October 2026", "September 2026", "August 2026", "July 2026", "June 2026", "May 2026", "December 2025",
    ]);
    const october = groups[0];
    expect(october.findAll("tr[id^='tx-']").map((r) => r.attributes("id"))).toEqual(["tx-1", "tx-2"]);
    expect(october.get("[data-testid='month-totals']").text()).toBe("out $300.00 · in $1,200.00");
    expect(groups[1].get("[data-testid='month-totals']").text()).toBe("out $25.00");
    expect(groups[2].get("[data-testid='month-totals']").text()).toBe("in $85.00");
  });

  it("month_totals_leave_out_voided_entries", async () => {
    const wrapper = await launch();
    await wrapper.get("input[type='checkbox']").setValue(true);
    await flushPromises();
    const june = wrapper.findAll("tbody[data-month]").find((g) => g.text().includes("June"))!;
    expect(june.findAll("tr[id^='tx-']")).toHaveLength(2);
    expect(june.get("[data-testid='month-totals']").text()).toBe("out $19.99");
  });

  it("column_widths_fit_the_day_and_two_amount_columns", async () => {
    const wrapper = await launch();
    const cols = wrapper.findAll("table colgroup col");
    expect(cols).toHaveLength(6);
    expect(cols[0].classes()).toContain("w-[4.25rem]");
    expect(cols[2].classes()).toEqual(expect.arrayContaining(["w-[11rem]", "lg:w-[13rem]"]));
    expect(cols[3].classes()).toEqual(expect.arrayContaining(["w-[9.5rem]", "lg:w-[11rem]"]));
    expect(cols[4].classes()).toContain("w-[6.75rem]");
    expect(cols[5].classes()).toContain("w-[6.75rem]");
  });

  it("s_ui_19_cells_expense_row", async () => {
    const wrapper = await launch();
    const cells = rowFor(wrapper, 1).findAll("td");
    expect(cells[0].get(".sr-only").text()).toBe("Oct 20, 2026");
    expect(cells[0].get("[aria-hidden='true']").findAll("span").map((x) => x.text())).toEqual(["20", "Tue"]);
    expect(cells[1].text()).toContain("Laptop for client work");
    expect(cells[1].get(".chip").text()).toBe("Asset");
    expect(cells[2].text()).toContain("Supplies");
    expect(cells[2].get(".line-tag").text()).toBe("Schedule C line L22");
    expect(cells[2].get(".line-tag").attributes("title")).toBe("Schedule C line 22");
    expect(cells[3].text()).toContain("Business Visa");
    expect(cells[3].get(".sr-only").text()).toContain("ending");
    expect(cells[3].get("[aria-hidden='true']").text()).toBe("\u00b7\u00b7");
    expect(cells[3].text()).toContain("\u00b7\u00b74242");
    expect(cells[4].text()).toBe("Paid out $300.00");
    expect(cells[4].classes()).toEqual(expect.arrayContaining(["text-right", "tabular-nums"]));
    expect(cells[5].text()).toBe("\u00b7");
    expect(cells[5].get("span").attributes("aria-hidden")).toBe("true");
  });

  it("s_ui_19_cells_income_row", async () => {
    const wrapper = await launch();
    const cells = rowFor(wrapper, 2).findAll("td");
    expect(cells[1].text()).toContain("Website build");
    expect(cells[1].text()).toContain("from Upwork");
    expect(cells[4].text()).toBe("\u00b7");
    expect(cells[5].text()).toBe("Received $1,200.00");
    expect(cells[5].classes()).toContain("text-income");
  });

  it("s_ui_19_paid_with_retired_and_not_set", async () => {
    const wrapper = await launch();
    const old = rowFor(wrapper, 9).findAll("td")[3];
    expect(old.text()).toContain("Old Amex");
    expect(old.get(".chip").text()).toBe("Retired");
    const none = rowFor(wrapper, 5).findAll("td")[3];
    expect(none.text()).toBe("Not set");
    expect(none.get("span").classes()).toContain("text-muted");
    for (const id of [1, 2, 4, 5, 9]) expect(rowFor(wrapper, id).text()).not.toContain("\u2014");
  });

  it("s_ui_20_voided_row_and_trail", async () => {
    const wrapper = await launch();
    await wrapper.get("input[type='checkbox']").setValue(true);
    await flushPromises();
    const voided = rowFor(wrapper, 6);
    const cells = voided.findAll("td");
    expect(cells[1].text()).toContain("Voided");
    expect(cells[1].get(".line-through").text()).toContain("Domain renewal");
    expect(cells[4].classes()).toContain("line-through");
    expect(cells[5].classes()).not.toContain("text-income");
    const trail = wrapper.get("#trail-6");
    expect(trail.text()).toMatch(/^Voided [A-Z][a-z]{2} \d{1,2}, \d{4}: Duplicate entry$/);
    expect(voided.attributes("aria-describedby")).toBe("trail-6");
    expect(rowFor(wrapper, 5).find(".line-through").exists()).toBe(false);
    expect(wrapper.find("#trail-5").exists()).toBe(false);
  });

  it("s_ui_20_corrected_row_links_both_ways", async () => {
    const wrapper = await launch();
    await wrapper.get("input[type='checkbox']").setValue(true);
    await flushPromises();
    const toReplacement = wrapper.get("#trail-7").get("button");
    expect(toReplacement.text()).toBe("Corrected by #8");
    expect(rowFor(wrapper, 8).findAll("td")[1].text()).toContain("Correction");
    const toOriginal = wrapper.get("#trail-8").get("button");
    expect(toOriginal.text()).toBe("Corrects #7");

    await toReplacement.trigger("click");
    await flushPromises();
    expect(document.activeElement).toBe(rowFor(wrapper, 8).element);
    await toOriginal.trigger("click");
    await flushPromises();
    expect(document.activeElement).toBe(rowFor(wrapper, 7).element);
  });

  it("s_ui_20_trail_button_emits_goto", () => {
    const rows = seedTransactions.filter((t) => t.id === 7);
    const wrapper = mount(LedgerTable, { props: { rows, categories } });
    return wrapper.get("#trail-7").get("button").trigger("click").then(() => {
      expect(wrapper.emitted("goto")?.[0]).toEqual([8]);
    });
  });

  it("focusing_a_row_flashes_it_for_1200ms", async () => {
    vi.useFakeTimers();
    try {
      const wrapper = mount(LedgerTable, { props: { rows: seedTransactions.slice(0, 1), categories }, attachTo: document.body });
      const row = wrapper.get("#tx-1");
      expect(row.attributes("data-flash")).toBeUndefined();
      (row.element as HTMLElement).focus();
      await wrapper.vm.$nextTick();
      expect(row.attributes("data-flash")).toBeDefined();
      vi.advanceTimersByTime(1199);
      await wrapper.vm.$nextTick();
      expect(row.attributes("data-flash")).toBeDefined();
      vi.advanceTimersByTime(2);
      await wrapper.vm.$nextTick();
      expect(row.attributes("data-flash")).toBeUndefined();
      wrapper.unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it("corrected_link_turns_on_show_voided_when_original_is_hidden", async () => {
    const wrapper = await launch();
    await wrapper.get("input[type='checkbox']").setValue(false);
    await flushPromises();
    expect(wrapper.find("#tx-7").exists()).toBe(false);

    await wrapper.get("#trail-8").get("button").trigger("click");
    await flushPromises();
    expect((wrapper.get("input[type='checkbox']").element as HTMLInputElement).checked).toBe(true);
    expect(document.activeElement).toBe(wrapper.get("#tx-7").element);
  });

  it("s_ui_20_reclassified_row_shows_history", async () => {
    const wrapper = await launch();
    expect(wrapper.get("#trail-4").text()).toContain(
      "Category changed from Needs a category to Office expense: Ink and paper are office supplies",
    );
    expect(rowFor(wrapper, 4).text()).toContain("Office expense");
  });

  it("uncategorized_is_written_out", () => {
    const row = { ...seedTransactions[4], category: "uncategorized", category_label: "Needs a category", schedule_c_line: null, classification_history: [] };
    const wrapper = mount(LedgerTable, { props: { rows: [row], categories } });
    expect(wrapper.text()).toContain("Needs a category");
    expect(wrapper.find(".line-tag").exists()).toBe(false);
  });
});
