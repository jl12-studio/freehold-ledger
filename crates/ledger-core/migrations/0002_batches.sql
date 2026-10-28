CREATE TABLE batches (
  id INTEGER PRIMARY KEY,
  label TEXT NOT NULL CHECK (length(trim(label)) > 0),
  recorded_at TEXT NOT NULL
);
CREATE TABLE transaction_batches (
  transaction_id INTEGER PRIMARY KEY REFERENCES transactions(id),
  batch_id INTEGER NOT NULL REFERENCES batches(id)
);
CREATE INDEX transaction_batches_batch_id ON transaction_batches(batch_id);

CREATE TRIGGER batches_no_update BEFORE UPDATE ON batches
BEGIN SELECT RAISE(ABORT, 'batches are append-only'); END;
CREATE TRIGGER batches_no_delete BEFORE DELETE ON batches
BEGIN SELECT RAISE(ABORT, 'batches are append-only'); END;
CREATE TRIGGER batches_no_replace BEFORE INSERT ON batches
WHEN EXISTS (SELECT 1 FROM batches WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'batches are append-only'); END;
CREATE TRIGGER transaction_batches_no_update BEFORE UPDATE ON transaction_batches
BEGIN SELECT RAISE(ABORT, 'transaction_batches are append-only'); END;
CREATE TRIGGER transaction_batches_no_delete BEFORE DELETE ON transaction_batches
BEGIN SELECT RAISE(ABORT, 'transaction_batches are append-only'); END;
CREATE TRIGGER transaction_batches_no_replace BEFORE INSERT ON transaction_batches
WHEN EXISTS (SELECT 1 FROM transaction_batches WHERE transaction_id = NEW.transaction_id)
BEGIN SELECT RAISE(ABORT, 'transaction_batches are append-only'); END;
