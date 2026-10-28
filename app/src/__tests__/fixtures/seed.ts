import type { Category, PaymentMethod, PaymentMethodRef, Transaction } from "../../lib/api";

const BUSINESS_VISA: PaymentMethodRef = { id: 1, kind: "card", nickname: "Business Visa", last4: "4242", retired: false };
const CHECKING: PaymentMethodRef = { id: 2, kind: "bank_account", nickname: "Checking", last4: "6789", retired: false };
const CASH: PaymentMethodRef = { id: 3, kind: "cash", nickname: "Cash", last4: null, retired: false };
const OLD_AMEX: PaymentMethodRef = { id: 4, kind: "card", nickname: "Old Amex", last4: "0005", retired: true };

// Shaped like the Rust serde output of the JIP-472 seed (newest first).
export const seedTransactions: Transaction[] = [
  {
    "id": 1,
    "kind": "expense",
    "occurred_on": "2026-10-20",
    "amount_cents": 30000,
    "description": "Laptop for client work",
    "original_category": "supplies",
    "category": "supplies",
    "category_label": "Supplies",
    "schedule_c_line": "22",
    "is_asset": true,
    "income_source": null,
    "payment_method": BUSINESS_VISA,
    "replaces_id": null,
    "replaced_by_id": null,
    "recorded_at": "2026-10-28T02:15:47.940Z",
    "void": null,
    "classification_history": []
  },
  {
    "id": 2,
    "kind": "income",
    "occurred_on": "2026-10-15",
    "amount_cents": 120000,
    "description": "Website build",
    "original_category": "gross_receipts",
    "category": "gross_receipts",
    "category_label": "Gross receipts or sales",
    "schedule_c_line": "1",
    "is_asset": false,
    "income_source": "Upwork",
    "payment_method": CHECKING,
    "replaces_id": null,
    "replaced_by_id": null,
    "recorded_at": "2026-10-28T02:15:47.940Z",
    "void": null,
    "classification_history": []
  },
  {
    "id": 10,
    "kind": "expense",
    "occurred_on": "2026-09-30",
    "amount_cents": 2500,
    "description": "Flyers",
    "original_category": "advertising",
    "category": "advertising",
    "category_label": "Advertising",
    "schedule_c_line": "8",
    "is_asset": false,
    "income_source": null,
    "payment_method": BUSINESS_VISA,
    "replaces_id": null,
    "replaced_by_id": null,
    "recorded_at": "2026-10-28T02:15:47.940Z",
    "void": null,
    "classification_history": []
  },
  {
    "id": 3,
    "kind": "income",
    "occurred_on": "2026-08-03",
    "amount_cents": 8500,
    "description": "Print sale",
    "original_category": "gross_receipts",
    "category": "gross_receipts",
    "category_label": "Gross receipts or sales",
    "schedule_c_line": "1",
    "is_asset": false,
    "income_source": "Etsy",
    "payment_method": CHECKING,
    "replaces_id": null,
    "replaced_by_id": null,
    "recorded_at": "2026-10-28T02:15:47.940Z",
    "void": null,
    "classification_history": []
  },
  {
    "id": 4,
    "kind": "expense",
    "occurred_on": "2026-07-12",
    "amount_cents": 4210,
    "description": "Printer ink and paper",
    "original_category": "uncategorized",
    "category": "office_expense",
    "category_label": "Office expense",
    "schedule_c_line": "18",
    "is_asset": false,
    "income_source": null,
    "payment_method": CASH,
    "replaces_id": null,
    "replaced_by_id": null,
    "recorded_at": "2026-10-28T02:15:47.940Z",
    "void": null,
    "classification_history": [
      {
        "category": "office_expense",
        "reason": "Ink and paper are office supplies",
        "recorded_at": "2026-10-28T02:15:47.940Z"
      }
    ]
  },
  {
    "id": 6,
    "kind": "expense",
    "occurred_on": "2026-06-01",
    "amount_cents": 1999,
    "description": "Domain renewal",
    "original_category": "advertising",
    "category": "advertising",
    "category_label": "Advertising",
    "schedule_c_line": "8",
    "is_asset": false,
    "income_source": null,
    "payment_method": null,
    "replaces_id": null,
    "replaced_by_id": null,
    "recorded_at": "2026-10-28T02:15:47.940Z",
    "void": {
      "reason": "Duplicate entry",
      "recorded_at": "2026-10-28T02:15:47.940Z"
    },
    "classification_history": []
  },
  {
    "id": 5,
    "kind": "expense",
    "occurred_on": "2026-06-01",
    "amount_cents": 1999,
    "description": "Domain renewal",
    "original_category": "advertising",
    "category": "advertising",
    "category_label": "Advertising",
    "schedule_c_line": "8",
    "is_asset": false,
    "income_source": null,
    "payment_method": null,
    "replaces_id": null,
    "replaced_by_id": null,
    "recorded_at": "2026-10-28T02:15:47.940Z",
    "void": null,
    "classification_history": []
  },
  {
    "id": 8,
    "kind": "expense",
    "occurred_on": "2026-05-05",
    "amount_cents": 30000,
    "description": "Monitor",
    "original_category": "supplies",
    "category": "supplies",
    "category_label": "Supplies",
    "schedule_c_line": "22",
    "is_asset": false,
    "income_source": null,
    "payment_method": null,
    "replaces_id": 7,
    "replaced_by_id": null,
    "recorded_at": "2026-10-28T02:15:47.940Z",
    "void": null,
    "classification_history": []
  },
  {
    "id": 7,
    "kind": "expense",
    "occurred_on": "2026-05-05",
    "amount_cents": 3000,
    "description": "Monitor",
    "original_category": "supplies",
    "category": "supplies",
    "category_label": "Supplies",
    "schedule_c_line": "22",
    "is_asset": false,
    "income_source": null,
    "payment_method": null,
    "replaces_id": null,
    "replaced_by_id": 8,
    "recorded_at": "2026-10-28T02:15:47.940Z",
    "void": {
      "reason": "Typo: was $300.00",
      "recorded_at": "2026-10-28T02:15:47.940Z"
    },
    "classification_history": []
  },
  {
    "id": 9,
    "kind": "expense",
    "occurred_on": "2025-12-15",
    "amount_cents": 90000,
    "description": "Booth rent, winter market",
    "original_category": "rent_other",
    "category": "rent_other",
    "category_label": "Rent or lease: other business property",
    "schedule_c_line": "20b",
    "is_asset": false,
    "income_source": null,
    "payment_method": OLD_AMEX,
    "replaces_id": null,
    "replaced_by_id": null,
    "recorded_at": "2026-10-28T02:15:47.940Z",
    "void": null,
    "classification_history": []
  }
];

// A subset of the real category list, enough for the seed and the filters.
export const categories: Category[] = [
  {
    "key": "advertising",
    "schedule_c_line": "8",
    "label": "Advertising",
    "kind": "expense"
  },
  {
    "key": "office_expense",
    "schedule_c_line": "18",
    "label": "Office expense",
    "kind": "expense"
  },
  {
    "key": "rent_other",
    "schedule_c_line": "20b",
    "label": "Rent or lease: other business property",
    "kind": "expense"
  },
  {
    "key": "supplies",
    "schedule_c_line": "22",
    "label": "Supplies",
    "kind": "expense"
  },
  {
    "key": "uncategorized",
    "schedule_c_line": null,
    "label": "Needs a category",
    "kind": "expense"
  },
  {
    "key": "gross_receipts",
    "schedule_c_line": "1",
    "label": "Gross receipts or sales",
    "kind": "income"
  },
  {
    "key": "other_income",
    "schedule_c_line": "6",
    "label": "Other income",
    "kind": "income"
  }
];

export const sources: string[] = ["Etsy", "Upwork"];

const pmMeta = { created_at: "2026-10-28T02:15:47.940Z" };
export const paymentMethods: PaymentMethod[] = [
  { ...BUSINESS_VISA, is_default: true, ...pmMeta, retired_at: null, transaction_count: 2 },
  { ...CASH, is_default: false, ...pmMeta, retired_at: null, transaction_count: 1 },
  { ...CHECKING, is_default: false, ...pmMeta, retired_at: null, transaction_count: 2 },
  { ...OLD_AMEX, is_default: false, ...pmMeta, retired_at: "2026-10-28T02:15:47.940Z", transaction_count: 1 },
];
