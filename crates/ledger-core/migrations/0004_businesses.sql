CREATE TABLE businesses (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL
    CHECK (length(trim(name)) BETWEEN 1 AND 60)
    CHECK (name NOT GLOB '*[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]*'),
  note TEXT CHECK (note IS NULL OR length(trim(note)) BETWEEN 1 AND 200),
  is_test INTEGER NOT NULL DEFAULT 0 CHECK (is_test IN (0,1)),
  created_at TEXT NOT NULL
);
CREATE TABLE business_changes (
  id INTEGER PRIMARY KEY,
  business_id INTEGER NOT NULL REFERENCES businesses(id),
  action TEXT NOT NULL CHECK (action IN ('rename','note','test','archive','restore')),
  name TEXT
    CHECK (name IS NULL OR length(trim(name)) BETWEEN 1 AND 60)
    CHECK (name IS NULL OR name NOT GLOB '*[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]*'),
  note TEXT CHECK (note IS NULL OR length(trim(note)) BETWEEN 1 AND 200),
  is_test INTEGER CHECK (is_test IS NULL OR is_test IN (0,1)),
  recorded_at TEXT NOT NULL,
  CHECK ((action = 'rename') = (name IS NOT NULL)),
  CHECK ((action = 'test') = (is_test IS NOT NULL)),
  CHECK (action = 'note' OR note IS NULL)
);
CREATE INDEX business_changes_business ON business_changes(business_id);
CREATE TABLE current_business_changes (
  id INTEGER PRIMARY KEY,
  business_id INTEGER REFERENCES businesses(id),
  recorded_at TEXT NOT NULL
);
CREATE TABLE transaction_businesses (
  transaction_id INTEGER PRIMARY KEY REFERENCES transactions(id),
  business_id INTEGER NOT NULL REFERENCES businesses(id)
);
CREATE INDEX transaction_businesses_business ON transaction_businesses(business_id);
CREATE TABLE batch_businesses (
  batch_id INTEGER PRIMARY KEY REFERENCES batches(id),
  business_id INTEGER NOT NULL REFERENCES businesses(id)
);
CREATE TABLE payment_method_businesses (
  payment_method_id INTEGER PRIMARY KEY REFERENCES payment_methods(id),
  business_id INTEGER NOT NULL REFERENCES businesses(id)
);
CREATE INDEX payment_method_businesses_business ON payment_method_businesses(business_id);
CREATE TABLE business_default_payment_method_changes (
  id INTEGER PRIMARY KEY,
  business_id INTEGER NOT NULL REFERENCES businesses(id),
  payment_method_id INTEGER REFERENCES payment_methods(id),
  recorded_at TEXT NOT NULL
);
CREATE INDEX business_default_payment_method_changes_business ON business_default_payment_method_changes(business_id);

-- Existing books move into one business. Nothing is updated or deleted.
INSERT INTO businesses (id, name, note, is_test, created_at)
  VALUES (1, 'My business', NULL, 0, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
INSERT INTO current_business_changes (business_id, recorded_at)
  VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
INSERT INTO transaction_businesses (transaction_id, business_id) SELECT id, 1 FROM transactions;
INSERT INTO batch_businesses (batch_id, business_id) SELECT id, 1 FROM batches;
INSERT INTO payment_method_businesses (payment_method_id, business_id) SELECT id, 1 FROM payment_methods;
INSERT INTO business_default_payment_method_changes (business_id, payment_method_id, recorded_at)
  SELECT 1, payment_method_id, recorded_at FROM default_payment_method_changes ORDER BY id DESC LIMIT 1;

CREATE TRIGGER businesses_no_update BEFORE UPDATE ON businesses
BEGIN SELECT RAISE(ABORT, 'businesses are append-only'); END;
CREATE TRIGGER businesses_no_delete BEFORE DELETE ON businesses
BEGIN SELECT RAISE(ABORT, 'businesses are append-only'); END;
CREATE TRIGGER businesses_no_replace BEFORE INSERT ON businesses
WHEN EXISTS (SELECT 1 FROM businesses WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'businesses are append-only'); END;

CREATE TRIGGER business_changes_no_update BEFORE UPDATE ON business_changes
BEGIN SELECT RAISE(ABORT, 'business_changes are append-only'); END;
CREATE TRIGGER business_changes_no_delete BEFORE DELETE ON business_changes
BEGIN SELECT RAISE(ABORT, 'business_changes are append-only'); END;
CREATE TRIGGER business_changes_no_replace BEFORE INSERT ON business_changes
WHEN EXISTS (SELECT 1 FROM business_changes WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'business_changes are append-only'); END;

CREATE TRIGGER current_business_changes_no_update BEFORE UPDATE ON current_business_changes
BEGIN SELECT RAISE(ABORT, 'current_business_changes are append-only'); END;
CREATE TRIGGER current_business_changes_no_delete BEFORE DELETE ON current_business_changes
BEGIN SELECT RAISE(ABORT, 'current_business_changes are append-only'); END;
CREATE TRIGGER current_business_changes_no_replace BEFORE INSERT ON current_business_changes
WHEN EXISTS (SELECT 1 FROM current_business_changes WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'current_business_changes are append-only'); END;

CREATE TRIGGER transaction_businesses_no_update BEFORE UPDATE ON transaction_businesses
BEGIN SELECT RAISE(ABORT, 'transaction_businesses are append-only'); END;
CREATE TRIGGER transaction_businesses_no_delete BEFORE DELETE ON transaction_businesses
BEGIN SELECT RAISE(ABORT, 'transaction_businesses are append-only'); END;
CREATE TRIGGER transaction_businesses_no_replace BEFORE INSERT ON transaction_businesses
WHEN EXISTS (SELECT 1 FROM transaction_businesses WHERE transaction_id = NEW.transaction_id)
BEGIN SELECT RAISE(ABORT, 'transaction_businesses are append-only'); END;

CREATE TRIGGER batch_businesses_no_update BEFORE UPDATE ON batch_businesses
BEGIN SELECT RAISE(ABORT, 'batch_businesses are append-only'); END;
CREATE TRIGGER batch_businesses_no_delete BEFORE DELETE ON batch_businesses
BEGIN SELECT RAISE(ABORT, 'batch_businesses are append-only'); END;
CREATE TRIGGER batch_businesses_no_replace BEFORE INSERT ON batch_businesses
WHEN EXISTS (SELECT 1 FROM batch_businesses WHERE batch_id = NEW.batch_id)
BEGIN SELECT RAISE(ABORT, 'batch_businesses are append-only'); END;

CREATE TRIGGER payment_method_businesses_no_update BEFORE UPDATE ON payment_method_businesses
BEGIN SELECT RAISE(ABORT, 'payment_method_businesses are append-only'); END;
CREATE TRIGGER payment_method_businesses_no_delete BEFORE DELETE ON payment_method_businesses
BEGIN SELECT RAISE(ABORT, 'payment_method_businesses are append-only'); END;
CREATE TRIGGER payment_method_businesses_no_replace BEFORE INSERT ON payment_method_businesses
WHEN EXISTS (SELECT 1 FROM payment_method_businesses WHERE payment_method_id = NEW.payment_method_id)
BEGIN SELECT RAISE(ABORT, 'payment_method_businesses are append-only'); END;

CREATE TRIGGER business_default_payment_method_changes_no_update BEFORE UPDATE ON business_default_payment_method_changes
BEGIN SELECT RAISE(ABORT, 'business_default_payment_method_changes are append-only'); END;
CREATE TRIGGER business_default_payment_method_changes_no_delete BEFORE DELETE ON business_default_payment_method_changes
BEGIN SELECT RAISE(ABORT, 'business_default_payment_method_changes are append-only'); END;
CREATE TRIGGER business_default_payment_method_changes_no_replace BEFORE INSERT ON business_default_payment_method_changes
WHEN EXISTS (SELECT 1 FROM business_default_payment_method_changes WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'business_default_payment_method_changes are append-only'); END;
