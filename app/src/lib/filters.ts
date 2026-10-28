import type { Filter, KindName } from "./api";

export interface FilterState {
  from: string;
  to: string;
  kind: "" | KindName;
  category: string;
  source: string;
  include_voided: boolean;
}

export const DEFAULT_FILTERS: FilterState = {
  from: "",
  to: "",
  kind: "",
  category: "",
  source: "",
  include_voided: true,
};

export function isDirty(f: FilterState): boolean {
  return (Object.keys(DEFAULT_FILTERS) as (keyof FilterState)[]).some((k) => f[k] !== DEFAULT_FILTERS[k]);
}

/** Empty fields are left out of what is sent. */
export function toFilter(f: FilterState): Filter {
  const out: Filter = { include_voided: f.include_voided };
  if (f.from) out.from = f.from;
  if (f.to) out.to = f.to;
  if (f.kind) out.kind = f.kind;
  if (f.category) out.category = f.category;
  if (f.source) out.source = f.source;
  return out;
}
