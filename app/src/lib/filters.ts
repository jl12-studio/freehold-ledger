import type { Filter, KindName } from "./api";

export type Period =
  | ""
  | "this_month"
  | "last_month"
  | "this_quarter"
  | "last_quarter"
  | "this_year"
  | "last_year"
  | "custom";

export interface FilterState {
  period: Period;
  from: string;
  to: string;
  kind: "" | KindName;
  category: string;
  source: string;
  payment_method: string;
  include_voided: boolean;
}

export const DEFAULT_FILTERS: FilterState = {
  period: "",
  from: "",
  to: "",
  kind: "",
  category: "",
  source: "",
  payment_method: "",
  include_voided: false,
};

export function isDirty(f: FilterState): boolean {
  return (Object.keys(DEFAULT_FILTERS) as (keyof FilterState)[]).some((k) => f[k] !== DEFAULT_FILTERS[k]);
}

function ymd(y: number, m: number, d: number): string {
  return `${String(y).padStart(4, "0")}-${String(m + 1).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
}

function monthRange(y: number, m: number): { from: string; to: string } {
  // Normalise month overflow/underflow, then take day 0 of the next month as the last day.
  const first = new Date(y, m, 1);
  const last = new Date(first.getFullYear(), first.getMonth() + 1, 0);
  return {
    from: ymd(first.getFullYear(), first.getMonth(), 1),
    to: ymd(last.getFullYear(), last.getMonth(), last.getDate()),
  };
}

/** The date range a named period covers, in local dates; "" and custom resolve to none here. */
export function periodRange(period: Period, today: Date): { from?: string; to?: string } {
  const y = today.getFullYear();
  const m = today.getMonth();
  switch (period) {
    case "this_month":
      return monthRange(y, m);
    case "last_month":
      return monthRange(y, m - 1);
    case "this_quarter":
    case "last_quarter": {
      const q = Math.floor(m / 3) + (period === "last_quarter" ? -1 : 0);
      const start = monthRange(y, q * 3);
      return { from: start.from, to: monthRange(y, q * 3 + 2).to };
    }
    case "this_year":
      return { from: ymd(y, 0, 1), to: ymd(y, 11, 31) };
    case "last_year":
      return { from: ymd(y - 1, 0, 1), to: ymd(y - 1, 11, 31) };
    default:
      return {};
  }
}

/** Empty fields are left out of what is sent. Voided rows are always fetched; the window hides them. */
export function toFilter(f: FilterState, businessId: number | null, today: Date = new Date()): Filter {
  const out: Filter = { include_voided: true };
  if (businessId !== null) out.business_id = businessId;
  const range = f.period === "custom" ? { from: f.from, to: f.to } : periodRange(f.period, today);
  if (range.from) out.from = range.from;
  if (range.to) out.to = range.to;
  if (f.kind) out.kind = f.kind;
  if (f.category) out.category = f.category;
  if (f.source) out.source = f.source;
  if (f.payment_method) out.payment_method = f.payment_method;
  return out;
}
