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

  it("s_ui_18_five_columns_single_line_rows", async () => {
    const wrapper = await launch();
    await wrapper.get("input[type='checkbox']").setValue(true);
    await flushPromises();
    expect(wrapper.get("table caption").text()).toBe("Ledger entries");
    expect(wrapper.findAll("th[scope='col']").map((h) => h.text())).toEqual([
      "Date", "Description", "Category", "Paid with", "Amount",
    ]);
    const rows = wrapper.findAll("tbody tr[id^='tx-']");
    expect(rows).toHaveLength(10);
    expect(rows[0].text()).toContain("Laptop for client work");
    expect(rows[9].text()).toContain("Booth rent, winter market");
    for (const r of rows) expect(r.findAll("td")).toHaveLength(5);
    expect(wrapper.get("table").classes()).toEqual(expect.arrayContaining(["table-fixed", "min-w-[42rem]"]));
  });

  it("date_column_is_wide_enough_for_its_text_plus_the_cell_padding", async () => {
    const wrapper = await launch();
    const cols = wrapper.findAll("table colgroup col");
    expect(cols).toHaveLength(5);
    expect(cols[0].classes()).toContain("w-[7.75rem]");
    expect(wrapper.get("tbody tr td").classes()).toEqual(expect.arrayContaining(["pr-4", "whitespace-nowrap"]));
  });

  it("s_ui_19_cells_expense_row", async () => {
    const wrapper = await launch();
    const cells = rowFor(wrapper, 1).findAll("td");
    expect(cells[0].text()).toBe("Oct 20, 2026");
    expect(cells[1].text()).toContain("Laptop for client work");
    expect(cells[1].get(".chip").text()).toBe("Asset");
    expect(cells[2].text()).toContain("Supplies");
    expect(cells[2].text()).toContain("line 22");
    expect(cells[3].text()).toContain("Business Visa");
    expect(cells[3].get(".sr-only").text()).toContain("ending");
    expect(cells[3].get("[aria-hidden='true']").text()).toBe("\u00b7\u00b7");
    expect(cells[3].text()).toContain("\u00b7\u00b74242");
    expect(cells[4].text()).toBe("Expense$300.00");
    expect(cells[4].get(".sr-only").text()).toBe("Expense");
    expect(cells[4].classes()).toEqual(expect.arrayContaining(["text-right", "tabular-nums"]));
  });

  it("s_ui_19_cells_income_row", async () => {
    const wrapper = await launch();
    const cells = rowFor(wrapper, 2).findAll("td");
    expect(cells[1].text()).toContain("Website build");
    expect(cells[1].text()).toContain("from Upwork");
    expect(cells[4].text()).toBe("Income+$1,200.00");
    expect(cells[4].get(".sr-only").text()).toBe("Income");
    expect(cells[4].classes()).toContain("text-income");
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
    expect(cells[4].classes()).not.toContain("text-income");
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
    expect(wrapper.text()).not.toContain("line ");
  });
});
