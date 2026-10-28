CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE income_sources (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE COLLATE NOCASE,
  created_at TEXT NOT NULL
);
CREATE TABLE transactions (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('expense','income')),
  occurred_on TEXT NOT NULL CHECK (occurred_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  amount_cents INTEGER NOT NULL CHECK (amount_cents > 0),
  description TEXT NOT NULL CHECK (length(trim(description)) > 0),
  category TEXT NOT NULL,
  is_asset INTEGER NOT NULL DEFAULT 0 CHECK (is_asset IN (0,1)),
  income_source_id INTEGER REFERENCES income_sources(id),
  replaces_id INTEGER REFERENCES transactions(id),
  recorded_at TEXT NOT NULL,
  CHECK ((kind = 'income') = (income_source_id IS NOT NULL)),
  CHECK (kind = 'expense' OR is_asset = 0)
);
CREATE INDEX transactions_occurred_on ON transactions(occurred_on);
CREATE TABLE voids (
  id INTEGER PRIMARY KEY,
  transaction_id INTEGER NOT NULL UNIQUE REFERENCES transactions(id),
  reason TEXT NOT NULL CHECK (length(trim(reason)) > 0),
  recorded_at TEXT NOT NULL
);
CREATE TABLE classification_changes (
  id INTEGER PRIMARY KEY,
  transaction_id INTEGER NOT NULL REFERENCES transactions(id),
  category TEXT NOT NULL,
  reason TEXT NOT NULL CHECK (length(trim(reason)) > 0),
  recorded_at TEXT NOT NULL
);

CREATE TRIGGER transactions_no_update BEFORE UPDATE ON transactions
BEGIN SELECT RAISE(ABORT, 'transactions are append-only'); END;
CREATE TRIGGER transactions_no_delete BEFORE DELETE ON transactions
BEGIN SELECT RAISE(ABORT, 'transactions are append-only'); END;
CREATE TRIGGER voids_no_update BEFORE UPDATE ON voids
BEGIN SELECT RAISE(ABORT, 'voids are append-only'); END;
CREATE TRIGGER voids_no_delete BEFORE DELETE ON voids
BEGIN SELECT RAISE(ABORT, 'voids are append-only'); END;
CREATE TRIGGER classification_changes_no_update BEFORE UPDATE ON classification_changes
BEGIN SELECT RAISE(ABORT, 'classification_changes are append-only'); END;
CREATE TRIGGER classification_changes_no_delete BEFORE DELETE ON classification_changes
BEGIN SELECT RAISE(ABORT, 'classification_changes are append-only'); END;
