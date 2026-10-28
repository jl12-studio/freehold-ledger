import type { Business } from "./api";

export function storageKey(path: string): string {
  return "freehold-ledger.view-business:" + path;
}

/** The remembered pick for this ledger file, or null when none or the store is blocked. */
export function loadViewBusiness(path: string): number | null {
  try {
    const raw = localStorage.getItem(storageKey(path));
    if (raw === null) return null;
    const id = Number(raw);
    return Number.isInteger(id) ? id : null;
  } catch {
    return null;
  }
}

export function saveViewBusiness(path: string, id: number): void {
  try {
    localStorage.setItem(storageKey(path), String(id));
  } catch {
    // A blocked store only means the pick is not remembered.
  }
}

/** Stored pick if it still exists, else the agent's current business, else first active, else first. */
export function pickBusiness(list: Business[], stored: number | null, currentId: number | null): Business | null {
  return (
    (stored !== null ? list.find((b) => b.id === stored) : undefined) ??
    (currentId !== null ? list.find((b) => b.id === currentId) : undefined) ??
    list.find((b) => !b.archived) ??
    list[0] ??
    null
  );
}
