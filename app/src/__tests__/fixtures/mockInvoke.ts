import { categories, paymentMethods, seedTransactions, sources } from "./seed";
import type { Filter } from "../../lib/api";

export const PATH = "/Users/pat/Ledger/freehold-ledger.sqlite3";

/** Mimics the Rust commands over the seed fixture (static data, no database). */
export function seedInvoke(cmd: string, args?: { filter?: Filter }) {
  if (cmd === "ledger_info") {
    return Promise.resolve({ path: PATH, schema_version: 1, transaction_count: seedTransactions.length });
  }
  if (cmd === "list_categories") return Promise.resolve(categories);
  if (cmd === "list_income_sources") return Promise.resolve(sources);
  if (cmd === "list_payment_methods") return Promise.resolve(paymentMethods);
  if (cmd === "list_transactions") {
    const f = args!.filter!;
    return Promise.resolve(
      seedTransactions.filter(
        (t) =>
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
