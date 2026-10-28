import { businesses, categories, paymentMethods, seedTransactions } from "./seed";
import type { Filter } from "../../lib/api";

export const PATH = "/Users/pat/Ledger/freehold-ledger.sqlite3";

type Args = { filter?: Filter; businessId?: number; year?: number };

/** Mimics the Rust commands over the seed fixture (static data, no database). */
export function seedInvoke(cmd: string, args?: Args) {
  if (cmd === "ledger_info") {
    return Promise.resolve({
      path: PATH,
      schema_version: 4,
      transaction_count: seedTransactions.length,
      current_business_id: 1,
    });
  }
  if (cmd === "list_businesses") return Promise.resolve(businesses);
  if (cmd === "list_categories") return Promise.resolve(categories);
  if (cmd === "list_income_sources") {
    const names = seedTransactions
      .filter((t) => t.business_id === args!.businessId && t.income_source)
      .map((t) => t.income_source as string);
    return Promise.resolve([...new Set(names)].sort());
  }
  if (cmd === "list_payment_methods") {
    return Promise.resolve(paymentMethods.filter((p) => p.business_id === args!.businessId));
  }
  if (cmd === "list_transactions") {
    const f = args!.filter!;
    return Promise.resolve(
      seedTransactions.filter(
        (t) =>
          (f.business_id === undefined || t.business_id === f.business_id) &&
          (!f.from || t.occurred_on >= f.from) &&
          (!f.to || t.occurred_on <= f.to) &&
          (!f.kind || t.kind === f.kind) &&
          (!f.category || t.category === f.category) &&
          (!f.source || t.income_source === f.source) &&
          (!f.payment_method || (f.payment_method === "none" ? !t.payment_method : String(t.payment_method?.id) === f.payment_method)) &&
          (f.include_voided || !t.void),
      ),
    );
  }
  return Promise.reject(new Error(`unexpected command ${cmd}`));
}
