import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { loadViewBusiness, pickBusiness, saveViewBusiness, storageKey } from "../lib/businessView";
import type { Business } from "../lib/api";
import { businesses } from "./fixtures/seed";

const PATH = "/Users/pat/Ledger/freehold-ledger.sqlite3";

describe("remembered business pick", () => {
  beforeEach(() => localStorage.clear());
  afterEach(() => vi.restoreAllMocks());

  it("s_ui_11_pick_is_stored_per_ledger_path_as_text", () => {
    saveViewBusiness(PATH, 2);
    expect(localStorage.getItem("freehold-ledger.view-business:" + PATH)).toBe("2");
    expect(storageKey(PATH)).toBe("freehold-ledger.view-business:" + PATH);
    expect(loadViewBusiness(PATH)).toBe(2);
    expect(loadViewBusiness("/other.sqlite3")).toBeNull();
  });

  it("s_ui_11_a_blocked_store_is_silent", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(loadViewBusiness(PATH)).toBeNull();
    expect(() => saveViewBusiness(PATH, 2)).not.toThrow();
  });

  it("pick_order_is_stored_then_current_then_first_active_then_first", () => {
    const archivedFirst: Business[] = [
      { ...businesses[0], id: 7, archived: true, is_current: false },
      { ...businesses[1], id: 8 },
    ];
    expect(pickBusiness(businesses, 2, 1)?.id).toBe(2);
    expect(pickBusiness(businesses, 99, 1)?.id).toBe(1);
    expect(pickBusiness(businesses, null, 2)?.id).toBe(2);
    expect(pickBusiness(archivedFirst, null, null)?.id).toBe(8);
    expect(pickBusiness([archivedFirst[0]], null, null)?.id).toBe(7);
    expect(pickBusiness(archivedFirst, 7, null)?.id).toBe(7);
    expect(pickBusiness([], null, null)).toBeNull();
  });
});
