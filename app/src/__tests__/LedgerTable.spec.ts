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

  it("s_ui_3_rows_render_with_tabular_amounts", async () => {
    const wrapper = await launch();

    const table = wrapper.get("table");
    expect(table.get("caption").text()).toBe("Ledger entries");
    const rows = wrapper.findAll("tbody tr[id^='tx-']");
    expect(rows).toHaveLength(10);
    // newest first
    expect(rows[0].text()).toContain("Laptop for client work");
    expect(rows[9].text()).toContain("Booth rent, winter market");

    const first = rows[0];
    expect(first.text()).toContain("Oct 20, 2026");
    expect(first.text()).toContain("Asset");
    expect(first.text()).toContain("Supplies");
    expect(first.text()).toContain("Sch. C line 22");
    expect(first.text()).toContain("Expense");
    const amount = first.findAll("td").slice(-1)[0];
    expect(amount.text()).toBe("$300.00");
    expect(amount.classes()).toEqual(expect.arrayContaining(["text-right", "tabular-nums"]));
    expect(rowFor(wrapper, 2).text()).toContain("$1,200.00");
    expect(rowFor(wrapper, 2).text()).toContain("Upwork");
    expect(rowFor(wrapper, 2).text()).toContain("Income");
    expect(wrapper.findAll("th[scope='col']").map((h) => h.text())).toEqual([
      "Date", "Description", "Category", "Source", "Kind", "Amount",
    ]);
  });

  it("s_ui_3_voided_row_struck_with_reason", async () => {
    const wrapper = await launch();
    const voided = rowFor(wrapper, 6);
    expect(voided.get(".line-through").text()).toContain("Domain renewal");
    expect(voided.findAll("td").slice(-1)[0].classes()).toContain("line-through");
    const trail = wrapper.get(`#trail-6`);
    expect(trail.text()).toContain("Voided");
    expect(trail.text()).toContain("Duplicate entry");
    expect(voided.attributes("aria-describedby")).toBe("trail-6");
    // the live twin is untouched
    expect(rowFor(wrapper, 5).find(".line-through").exists()).toBe(false);
  });

  it("s_ui_3_corrected_row_links_both_ways", async () => {
    const wrapper = await launch();
    const toReplacement = wrapper.get("#trail-7").get("button");
    expect(toReplacement.text()).toBe("Corrected by entry #8");
    const toOriginal = wrapper.get("#trail-8").get("button");
    expect(toOriginal.text()).toBe("Corrects entry #7");

    await toReplacement.trigger("click");
    await flushPromises();
    expect(document.activeElement).toBe(rowFor(wrapper, 8).element);
    await toOriginal.trigger("click");
    await flushPromises();
    expect(document.activeElement).toBe(rowFor(wrapper, 7).element);
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

  it("s_ui_3_reclassified_row_shows_history", async () => {
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
    expect(wrapper.text()).not.toContain("Sch. C");
  });
});
