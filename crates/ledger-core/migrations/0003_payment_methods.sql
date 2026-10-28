CREATE TABLE payment_methods (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('card','bank_account','cash','other')),
  nickname TEXT NOT NULL
    CHECK (length(trim(nickname)) BETWEEN 1 AND 40)
    CHECK (nickname NOT GLOB '*[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]*'),
  last4 TEXT CHECK (last4 IS NULL OR (length(last4) = 4 AND last4 GLOB '[0-9][0-9][0-9][0-9]')),
  created_at TEXT NOT NULL,
  CHECK (kind <> 'cash' OR last4 IS NULL)
);
CREATE TABLE payment_method_changes (
  id INTEGER PRIMARY KEY,
  payment_method_id INTEGER NOT NULL REFERENCES payment_methods(id),
  action TEXT NOT NULL CHECK (action IN ('rename','retire')),
  nickname TEXT
    CHECK (nickname IS NULL OR length(trim(nickname)) BETWEEN 1 AND 40)
    CHECK (nickname IS NULL OR nickname NOT GLOB '*[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]*'),
  recorded_at TEXT NOT NULL,
  CHECK ((action = 'rename') = (nickname IS NOT NULL))
);
CREATE INDEX payment_method_changes_method ON payment_method_changes(payment_method_id);
CREATE TABLE default_payment_method_changes (
  id INTEGER PRIMARY KEY,
  payment_method_id INTEGER REFERENCES payment_methods(id),
  recorded_at TEXT NOT NULL
);
CREATE TABLE payment_method_assignments (
  id INTEGER PRIMARY KEY,
  transaction_id INTEGER NOT NULL REFERENCES transactions(id),
  payment_method_id INTEGER NOT NULL REFERENCES payment_methods(id),
  recorded_at TEXT NOT NULL
);
CREATE INDEX payment_method_assignments_transaction ON payment_method_assignments(transaction_id);

CREATE TRIGGER payment_methods_no_update BEFORE UPDATE ON payment_methods
BEGIN SELECT RAISE(ABORT, 'payment_methods are append-only'); END;
CREATE TRIGGER payment_methods_no_delete BEFORE DELETE ON payment_methods
BEGIN SELECT RAISE(ABORT, 'payment_methods are append-only'); END;
CREATE TRIGGER payment_methods_no_replace BEFORE INSERT ON payment_methods
WHEN EXISTS (SELECT 1 FROM payment_methods WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'payment_methods are append-only'); END;

CREATE TRIGGER payment_method_changes_no_update BEFORE UPDATE ON payment_method_changes
BEGIN SELECT RAISE(ABORT, 'payment_method_changes are append-only'); END;
CREATE TRIGGER payment_method_changes_no_delete BEFORE DELETE ON payment_method_changes
BEGIN SELECT RAISE(ABORT, 'payment_method_changes are append-only'); END;
CREATE TRIGGER payment_method_changes_no_replace BEFORE INSERT ON payment_method_changes
WHEN EXISTS (SELECT 1 FROM payment_method_changes WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'payment_method_changes are append-only'); END;

CREATE TRIGGER default_payment_method_changes_no_update BEFORE UPDATE ON default_payment_method_changes
BEGIN SELECT RAISE(ABORT, 'default_payment_method_changes are append-only'); END;
CREATE TRIGGER default_payment_method_changes_no_delete BEFORE DELETE ON default_payment_method_changes
BEGIN SELECT RAISE(ABORT, 'default_payment_method_changes are append-only'); END;
CREATE TRIGGER default_payment_method_changes_no_replace BEFORE INSERT ON default_payment_method_changes
WHEN EXISTS (SELECT 1 FROM default_payment_method_changes WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'default_payment_method_changes are append-only'); END;

CREATE TRIGGER payment_method_assignments_no_update BEFORE UPDATE ON payment_method_assignments
BEGIN SELECT RAISE(ABORT, 'payment_method_assignments are append-only'); END;
CREATE TRIGGER payment_method_assignments_no_delete BEFORE DELETE ON payment_method_assignments
BEGIN SELECT RAISE(ABORT, 'payment_method_assignments are append-only'); END;
CREATE TRIGGER payment_method_assignments_no_replace BEFORE INSERT ON payment_method_assignments
WHEN EXISTS (SELECT 1 FROM payment_method_assignments WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'payment_method_assignments are append-only'); END;
