import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ revealItemInDir: vi.fn() }));

import App from "../App.vue";
import { PATH, seedInvoke } from "./fixtures/mockInvoke";
import { businesses } from "./fixtures/seed";

const KEY = "freehold-ledger.view-business:" + PATH;

async function launch() {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}
const ids = (w: ReturnType<typeof mount>) =>
  w.findAll("tbody tr[id^='tx-']").map((r) => Number(r.attributes("id")!.slice(3)));
const select = (w: ReturnType<typeof mount>) => w.get("#business-select");
const callsOf = (name: string) => invoke.mock.calls.filter((c) => c[0] === name);

describe("Business switcher in the window", () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockImplementation(seedInvoke);
    localStorage.clear();
    document.body.innerHTML = "";
  });
  afterEach(() => vi.restoreAllMocks());

  it("s_ui_9_opens_on_the_agents_current_business", async () => {
    const wrapper = await launch();

    const el = select(wrapper).element as HTMLSelectElement;
    expect(el.selectedOptions[0].text).toBe("My business");
    expect(wrapper.text()).toContain("Your agent records here");
    await wrapper.get("input[type='checkbox']").setValue(true);
    await flushPromises();
    expect(ids(wrapper)).toHaveLength(10);
    expect(wrapper.find(".chip-test").exists()).toBe(false);
    expect(wrapper.get("label[for='business-select']").text()).toBe("Business");
    expect(select(wrapper).findAll("option").map((o) => o.text())).toEqual(["My business", "Test shop (test)"]);
  });

  it("s_ui_10_picking_test_shop_scopes_everything_and_is_remembered", async () => {
    const wrapper = await launch();
    await select(wrapper).setValue("2");
    await flushPromises();

    expect(ids(wrapper)).toEqual([12, 11]);
    expect(wrapper.get(".chip-test").text()).toBe("Test");
    expect(wrapper.text()).toContain("Your agent records in My business");
    expect(localStorage.getItem(KEY)).toBe("2");
    expect(invoke).toHaveBeenCalledWith("list_income_sources", { businessId: 2 });
    expect(invoke).toHaveBeenCalledWith("list_payment_methods", { businessId: 2 });
    expect(callsOf("list_transactions").pop()![1].filter.business_id).toBe(2);
  });

  it("s_ui_11_a_stored_pick_is_restored_on_remount", async () => {
    localStorage.setItem(KEY, "2");
    const wrapper = await launch();

    expect((select(wrapper).element as HTMLSelectElement).selectedOptions[0].text).toBe("Test shop (test)");
    expect(ids(wrapper)).toEqual([12, 11]);
    expect(wrapper.find(".chip-test").exists()).toBe(true);
  });

  it("s_ui_11_a_stored_business_that_is_gone_falls_back_to_current", async () => {
    localStorage.setItem(KEY, "99");
    const wrapper = await launch();

    expect((select(wrapper).element as HTMLSelectElement).selectedOptions[0].text).toBe("My business");
  });

  it("s_ui_11_a_blocked_store_still_opens_on_the_current_business", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const wrapper = await launch();

    expect((select(wrapper).element as HTMLSelectElement).selectedOptions[0].text).toBe("My business");
    expect(wrapper.find("[role='alert']").exists()).toBe(false);
    expect(wrapper.text()).not.toContain("could not be opened");
    await select(wrapper).setValue("2");
    await flushPromises();
    expect(ids(wrapper)).toEqual([12, 11]);
  });

  it("s_ui_12_switching_clears_source_and_payment_method_but_keeps_the_rest", async () => {
    localStorage.setItem(KEY, "2");
    const wrapper = await launch();

    await wrapper.get("select[aria-label='Payment method']").setValue("5");
    await flushPromises();
    await wrapper.get("input[type='radio'][name='kind']:not([value=''])").setValue(true);
    await flushPromises();
    expect(callsOf("list_transactions").pop()![1].filter).toMatchObject({ payment_method: "5", kind: "expense", business_id: 2 });

    await select(wrapper).setValue("1");
    await flushPromises();

    const filter = callsOf("list_transactions").pop()![1].filter;
    expect(filter.payment_method).toBeUndefined();
    expect(filter.source).toBeUndefined();
    expect(filter.kind).toBe("expense");
    expect(filter.business_id).toBe(1);
    expect(ids(wrapper).length).toBeGreaterThan(0);
  });

  it("s_ui_13_the_window_only_ever_reads", async () => {
    const wrapper = await launch();
    await select(wrapper).setValue("2");
    await flushPromises();
    invoke.mockImplementation((cmd: string, args?: never) =>
      cmd === "ledger_years"
        ? Promise.resolve([2026])
        : cmd === "export_tax_year"
          ? Promise.resolve({ folder: "/x/Freehold Ledger Test shop (test) 2026 export", files: [], business: "Test shop" })
          : seedInvoke(cmd, args),
    );
    await wrapper.findAll("button").find((b) => b.text() === "Export")!.trigger("click");
    await flushPromises();

    const allowed = new Set([
      "ledger_info",
      "list_businesses",
      "list_categories",
      "list_income_sources",
      "list_payment_methods",
      "list_transactions",
      "ledger_years",
      "year_entries",
      "export_tax_year",
      "mcp_connection_info",
    ]);
    const names = invoke.mock.calls.map((c) => c[0] as string);
    expect(names.length).toBeGreaterThan(5);
    expect(names.filter((n) => !allowed.has(n))).toEqual([]);
    expect(invoke).toHaveBeenCalledWith("export_tax_year", { year: 2026, businessId: 2 });
    expect(wrapper.get("[role='status']").text()).toContain("Exported Test shop 2026 to");
  });

  it("s_ui_14_an_empty_business_that_is_not_current_teaches_how_to_switch", async () => {
    const newShop = { ...businesses[0], id: 3, name: "New shop", is_current: false, transaction_count: 0 };
    invoke.mockImplementation((cmd: string, args?: never) =>
      cmd === "list_businesses" ? Promise.resolve([...businesses, newShop]) : seedInvoke(cmd, args),
    );
    const wrapper = await launch();
    await select(wrapper).setValue("3");
    await flushPromises();

    expect(wrapper.text()).toContain("Nothing recorded in New shop yet");
    expect(wrapper.text()).toContain("Your agent records in My business. To record here, tell it: “switch to New shop”.");
    expect(wrapper.find("table").exists()).toBe(false);
  });

  it("s_ui_14_an_empty_current_business_says_tell_your_agent", async () => {
    invoke.mockImplementation((cmd: string, args?: never) =>
      cmd === "list_businesses" ? Promise.resolve([{ ...businesses[0], transaction_count: 0 }, businesses[1]]) : seedInvoke(cmd, args),
    );
    const wrapper = await launch();

    expect(wrapper.text()).toContain("Nothing recorded in My business yet");
    expect(wrapper.text()).toContain("Tell your agent what you bought or earned and it lands here.");
  });

  it("s_ui_14_a_single_business_shows_its_name_as_text", async () => {
    invoke.mockImplementation((cmd: string, args?: never) =>
      cmd === "list_businesses" ? Promise.resolve([businesses[0]]) : seedInvoke(cmd, args),
    );
    const wrapper = await launch();

    expect(wrapper.find("#business-select").exists()).toBe(false);
    expect(wrapper.text()).toContain("My business");
    expect(wrapper.text()).toContain("Business");
  });

  it("archived_and_no_current_business_captions", async () => {
    const archived = { ...businesses[1], id: 4, name: "Old shop", is_test: false, archived: true, archived_at: "2026-10-01T00:00:00Z" };
    invoke.mockImplementation((cmd: string, args?: never) => {
      if (cmd === "list_businesses") return Promise.resolve([{ ...businesses[0], is_current: false }, archived]);
      if (cmd === "ledger_info")
        return Promise.resolve({ path: PATH, schema_version: 4, transaction_count: 12, current_business_id: null });
      return seedInvoke(cmd, args);
    });
    const wrapper = await launch();
    expect(wrapper.text()).toContain("No business is current for your agent");
    const group = select(wrapper).get("optgroup");
    expect(group.attributes("label")).toBe("Archived");
    expect(group.findAll("option").map((o) => o.text())).toEqual(["Old shop"]);

    await select(wrapper).setValue("4");
    await flushPromises();
    expect(wrapper.get(".chip-muted").text()).toBe("Archived");
  });
});
