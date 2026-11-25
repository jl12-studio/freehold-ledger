import type { Transaction } from "./api";

export const MONTHS = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** Year, month (0-11), day and weekday of a "YYYY-MM-DD" date, independent of the viewer's time zone. */
export function dateParts(iso: string): { year: number; month: number; day: number; weekday: string } {
  const [year, m, day] = iso.slice(0, 10).split("-").map(Number);
  return { year, month: m - 1, day, weekday: WEEKDAYS[new Date(Date.UTC(year, m - 1, day)).getUTCDay()] };
}

/** First and last day of a month (0-11) as "YYYY-MM-DD". */
export function monthRange(year: number, month: number): { from: string; to: string } {
  const last = new Date(Date.UTC(year, month + 1, 0)).getUTCDate();
  const mm = String(month + 1).padStart(2, "0");
  return { from: `${year}-${mm}-01`, to: `${year}-${mm}-${String(last).padStart(2, "0")}` };
}

export interface Totals {
  in_cents: number;
  out_cents: number;
}

/** Income and expense totals, voided entries left out. */
export function totals(rows: Transaction[]): Totals {
  const t: Totals = { in_cents: 0, out_cents: 0 };
  for (const r of rows) {
    if (r.void) continue;
    if (r.kind === "income") t.in_cents += r.amount_cents;
    else t.out_cents += r.amount_cents;
  }
  return t;
}

/** Twelve monthly totals (January first) for entries dated in `year`. */
export function monthlyTotals(rows: Transaction[], year: number): Totals[] {
  const out = Array.from({ length: 12 }, () => ({ in_cents: 0, out_cents: 0 }));
  for (const r of rows) {
    if (r.void) continue;
    const p = dateParts(r.occurred_on);
    if (p.year !== year) continue;
    if (r.kind === "income") out[p.month].in_cents += r.amount_cents;
    else out[p.month].out_cents += r.amount_cents;
  }
  return out;
}

export interface MonthGroup {
  key: string;
  year: number;
  month: number;
  rows: Transaction[];
  totals: Totals;
}

/** Consecutive rows that share a calendar month, in the order given (the ledger lists newest first). */
export function groupByMonth(rows: Transaction[]): MonthGroup[] {
  const groups: MonthGroup[] = [];
  for (const r of rows) {
    const p = dateParts(r.occurred_on);
    const key = `${p.year}-${p.month}`;
    let g = groups[groups.length - 1];
    if (!g || g.key !== key) {
      g = { key, year: p.year, month: p.month, rows: [], totals: { in_cents: 0, out_cents: 0 } };
      groups.push(g);
    }
    g.rows.push(r);
  }
  for (const g of groups) g.totals = totals(g.rows);
  return groups;
}

export interface LineTotal {
  /** Schedule C line ("1", "20b"), or null for a category with no line. */
  line: string | null;
  label: string;
  kind: "expense" | "income";
  total_cents: number;
  count: number;
  assets: number;
}

/** Sorts "8" < "20a" < "20b" < "22"; entries with no line go last. */
function lineOrder(line: string | null): number {
  if (line === null) return Number.POSITIVE_INFINITY;
  const m = /^(\d+)([a-z]?)$/i.exec(line);
  if (!m) return Number.MAX_SAFE_INTEGER;
  return Number(m[1]) + (m[2] ? (m[2].toLowerCase().charCodeAt(0) - 96) / 100 : 0);
}

/** Live entries added up by Schedule C line; income lines first, then expense lines in line order. */
export function scheduleCTotals(rows: Transaction[]): LineTotal[] {
  const by = new Map<string, LineTotal>();
  for (const r of rows) {
    if (r.void) continue;
    const key = `${r.kind}:${r.schedule_c_line ?? `cat:${r.category}`}`;
    let t = by.get(key);
    if (!t) {
      t = { line: r.schedule_c_line, label: r.category_label, kind: r.kind, total_cents: 0, count: 0, assets: 0 };
      by.set(key, t);
    }
    t.total_cents += r.amount_cents;
    t.count += 1;
    if (r.is_asset) t.assets += 1;
  }
  return [...by.values()].sort(
    (a, b) =>
      (a.kind === b.kind ? 0 : a.kind === "income" ? -1 : 1) ||
      lineOrder(a.line) - lineOrder(b.line) ||
      a.label.localeCompare(b.label),
  );
}
