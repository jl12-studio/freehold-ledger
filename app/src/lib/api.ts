import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

export interface LedgerInfo {
  path: string;
  schema_version: number;
  transaction_count: number;
}

export interface AppError {
  kind: "not_a_ledger" | "newer_schema" | "io" | "invalid_input" | "other";
  message: string;
  path: string;
}

export function getLedgerInfo(): Promise<LedgerInfo> {
  return invoke<LedgerInfo>("ledger_info");
}

export function revealLedger(path: string): Promise<void> {
  return revealItemInDir(path);
}

export interface ExportResult {
  folder: string;
  files: string[];
}

export function listLedgerYears(): Promise<number[]> {
  return invoke<number[]>("ledger_years");
}

export function exportTaxYear(year: number): Promise<ExportResult> {
  return invoke<ExportResult>("export_tax_year", { year });
}

export function revealFolder(path: string): Promise<void> {
  return revealItemInDir(path);
}

export type KindName = "expense" | "income";

export interface VoidInfo {
  reason: string;
  recorded_at: string;
}

export interface ClassificationChange {
  category: string;
  reason: string;
  recorded_at: string;
}

export type PaymentKind = "card" | "bank_account" | "cash" | "other";

export interface PaymentMethodRef {
  id: number;
  kind: PaymentKind;
  nickname: string;
  last4: string | null;
  retired: boolean;
}

export interface PaymentMethod extends PaymentMethodRef {
  is_default: boolean;
  created_at: string;
  retired_at: string | null;
  transaction_count: number;
}

const KIND_LABEL: Record<PaymentKind, string> = {
  card: "Card",
  bank_account: "Bank account",
  cash: "Cash",
  other: "Other",
};

export function paymentCaption(p: PaymentMethodRef): string {
  return KIND_LABEL[p.kind] + (p.last4 ? ` ending ${p.last4}` : "") + (p.retired ? " (retired)" : "");
}

export interface Transaction {
  id: number;
  kind: KindName;
  occurred_on: string;
  amount_cents: number;
  description: string;
  original_category: string;
  category: string;
  category_label: string;
  schedule_c_line: string | null;
  is_asset: boolean;
  income_source: string | null;
  payment_method: PaymentMethodRef | null;
  replaces_id: number | null;
  replaced_by_id: number | null;
  recorded_at: string;
  void: VoidInfo | null;
  classification_history: ClassificationChange[];
}

export interface Category {
  key: string;
  schedule_c_line: string | null;
  label: string;
  kind: KindName;
}

/** What the window sends: empty fields are omitted. */
export interface Filter {
  from?: string;
  to?: string;
  kind?: KindName;
  category?: string;
  source?: string;
  payment_method?: string;
  include_voided: boolean;
}

export function listTransactions(filter: Filter): Promise<Transaction[]> {
  return invoke<Transaction[]>("list_transactions", { filter });
}

export function listCategories(): Promise<Category[]> {
  return invoke<Category[]>("list_categories");
}

export function listPaymentMethods(): Promise<PaymentMethod[]> {
  return invoke<PaymentMethod[]>("list_payment_methods");
}

export function listIncomeSources(): Promise<string[]> {
  return invoke<string[]>("list_income_sources");
}

const usd = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });

export function formatCents(cents: number): string {
  return usd.format(cents / 100);
}

const dateFmt = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric", year: "numeric" });

/** "2026-10-20" -> "Oct 20, 2026", independent of the viewer's time zone. */
export function formatDate(iso: string): string {
  const [y, m, d] = iso.slice(0, 10).split("-").map(Number);
  if (!y || !m || !d) return iso;
  return dateFmt.format(new Date(y, m - 1, d));
}

export interface ConnectionInfo {
  server_path: string;
  bundled: boolean;
  ledger_path: string;
  ledger_path_overridden: boolean;
  claude_desktop_json: string;
  claude_code_command: string;
  generic: string;
  agent_message: string;
}

export function getConnectionInfo(): Promise<ConnectionInfo> {
  return invoke<ConnectionInfo>("mcp_connection_info");
}
