use crate::categories::category;
use crate::db::Ledger;
use crate::error::LedgerError;
use crate::model::*;
use chrono::{NaiveDate, SecondsFormat};
use rusqlite::{params_from_iter, Connection, OptionalExtension, TransactionBehavior};

type Result<T> = std::result::Result<T, LedgerError>;

pub fn parse_date(s: &str) -> Result<NaiveDate> {
    let t = s.trim();
    let well_formed = t.len() == 10 && t.bytes().enumerate().all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() });
    if !well_formed {
        return Err(LedgerError::InvalidDate(s.to_string()));
    }
    NaiveDate::parse_from_str(t, "%Y-%m-%d").map_err(|_| LedgerError::InvalidDate(s.to_string()))
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

fn check_amount(a: i64) -> Result<()> {
    if a > 0 { Ok(()) } else { Err(LedgerError::InvalidAmount(a)) }
}

fn check_text(s: &str, e: LedgerError) -> Result<String> {
    let t = s.trim();
    if t.is_empty() {
        return Err(e);
    }
    let field = match e {
        LedgerError::MissingDescription => "description",
        LedgerError::MissingReason => "reason",
        LedgerError::MissingSource => "source",
        LedgerError::MissingBatchLabel => "label",
        _ => "text",
    };
    crate::guard::check_no_full_number(field, t)?;
    Ok(t.to_string())
}

fn check_category(key: &str, kind: Kind) -> Result<()> {
    match category(key) {
        Some(c) if c.kind == kind => Ok(()),
        _ => Err(LedgerError::UnknownCategory { key: key.to_string(), kind }),
    }
}

struct Row {
    kind: Kind,
    occurred_on: NaiveDate,
    amount_cents: i64,
    description: String,
    category: String,
    is_asset: bool,
    source: Option<String>,
}

fn insert(
    tx: &rusqlite::Transaction,
    row: &Row,
    replaces_id: Option<i64>,
) -> Result<i64> {
    let source_id: Option<i64> = match &row.source {
        Some(name) => {
            tx.execute(
                "INSERT OR IGNORE INTO income_sources (name, created_at) VALUES (?1, ?2)",
                rusqlite::params![name, now()],
            )?;
            Some(tx.query_row("SELECT id FROM income_sources WHERE name = ?1", [name], |r| r.get(0))?)
        }
        None => None,
    };
    tx.execute(
        "INSERT INTO transactions (kind, occurred_on, amount_cents, description, category, is_asset, income_source_id, replaces_id, recorded_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            row.kind.as_str(),
            row.occurred_on.format("%Y-%m-%d").to_string(),
            row.amount_cents,
            row.description,
            row.category,
            row.is_asset,
            source_id,
            replaces_id,
            now()
        ],
    )?;
    Ok(tx.last_insert_rowid())
}

const BASE: &str = "SELECT t.id, t.kind, t.occurred_on, t.amount_cents, t.description, t.category, \
    COALESCE((SELECT c.category FROM classification_changes c WHERE c.transaction_id = t.id ORDER BY c.id DESC LIMIT 1), t.category) AS eff, \
    t.is_asset, s.name AS src, t.replaces_id, \
    (SELECT r.id FROM transactions r WHERE r.replaces_id = t.id ORDER BY r.id LIMIT 1) AS replaced_by, \
    tb.batch_id, t.recorded_at, v.reason, v.recorded_at AS void_at \
    FROM transactions t \
    LEFT JOIN income_sources s ON s.id = t.income_source_id \
    LEFT JOIN voids v ON v.transaction_id = t.id \
    LEFT JOIN transaction_batches tb ON tb.transaction_id = t.id";

fn map_row(conn: &Connection, r: &rusqlite::Row) -> Result<Transaction> {
    let id: i64 = r.get("id")?;
    let kind = match r.get::<_, String>("kind")?.as_str() {
        "income" => Kind::Income,
        _ => Kind::Expense,
    };
    let occurred: String = r.get("occurred_on")?;
    let eff: String = r.get("eff")?;
    let cat = category(&eff);
    let reason: Option<String> = r.get("reason")?;
    let void = match reason {
        Some(reason) => Some(VoidInfo { reason, recorded_at: r.get("void_at")? }),
        None => None,
    };
    let mut stmt = conn.prepare(
        "SELECT category, reason, recorded_at FROM classification_changes WHERE transaction_id = ?1 ORDER BY id",
    )?;
    let history = stmt
        .query_map([id], |h| {
            Ok(ClassificationChange { category: h.get(0)?, reason: h.get(1)?, recorded_at: h.get(2)? })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(Transaction {
        id,
        kind,
        occurred_on: NaiveDate::parse_from_str(&occurred, "%Y-%m-%d")
            .map_err(|_| LedgerError::InvalidDate(occurred.clone()))?,
        amount_cents: r.get("amount_cents")?,
        description: r.get("description")?,
        original_category: r.get("category")?,
        category_label: cat.map(|c| c.label.to_string()).unwrap_or_else(|| eff.clone()),
        schedule_c_line: cat.and_then(|c| c.schedule_c_line.map(str::to_string)),
        category: eff,
        is_asset: r.get("is_asset")?,
        income_source: r.get("src")?,
        replaces_id: r.get("replaces_id")?,
        replaced_by_id: r.get("replaced_by")?,
        batch_id: r.get("batch_id")?,
        recorded_at: r.get("recorded_at")?,
        void,
        classification_history: history,
    })
}

fn load(conn: &Connection, id: i64) -> Result<Transaction> {
    let sql = format!("{BASE} WHERE t.id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([id])?;
    match rows.next()? {
        Some(r) => map_row(conn, r),
        None => Err(LedgerError::NotFound(id)),
    }
}

fn expense_row(e: NewExpense) -> Result<Row> {
    check_amount(e.amount_cents)?;
    let description = check_text(&e.description, LedgerError::MissingDescription)?;
    check_category(&e.category, Kind::Expense)?;
    Ok(Row {
        kind: Kind::Expense,
        occurred_on: e.occurred_on.unwrap_or_else(today),
        amount_cents: e.amount_cents,
        description,
        category: e.category,
        is_asset: e.is_asset,
        source: None,
    })
}

fn income_row(i: NewIncome) -> Result<Row> {
    check_amount(i.amount_cents)?;
    let description = check_text(&i.description, LedgerError::MissingDescription)?;
    let category = i.category.unwrap_or_else(|| "gross_receipts".to_string());
    check_category(&category, Kind::Income)?;
    let source = check_text(&i.source, LedgerError::MissingSource)?;
    Ok(Row {
        kind: Kind::Income,
        occurred_on: i.occurred_on.unwrap_or_else(today),
        amount_cents: i.amount_cents,
        description,
        category,
        is_asset: false,
        source: Some(source),
    })
}

impl Ledger {
    pub fn record_expense(&mut self, e: NewExpense) -> Result<Transaction> {
        let row = expense_row(e)?;
        self.insert_one(&row)
    }

    pub fn record_income(&mut self, i: NewIncome) -> Result<Transaction> {
        let row = income_row(i)?;
        self.insert_one(&row)
    }

    /// Records up to `MAX_BATCH` entries all or nothing, skipping entries that match a live one.
    pub fn record_batch(&mut self, b: NewBatch) -> Result<BatchResult> {
        let label = check_text(&b.label, LedgerError::MissingBatchLabel)?;
        if b.entries.is_empty() {
            return Err(LedgerError::EmptyBatch);
        }
        if b.entries.len() > MAX_BATCH {
            return Err(LedgerError::BatchTooLarge { count: b.entries.len() });
        }
        let mut rows = Vec::with_capacity(b.entries.len());
        for (i, be) in b.entries.into_iter().enumerate() {
            let row = match be.entry {
                NewEntry::Expense(e) => expense_row(e),
                NewEntry::Income(n) => income_row(n),
            }
            .map_err(|e| LedgerError::BatchEntry { index: i + 1, reason: Box::new(e) })?;
            rows.push((row, be.allow_duplicate));
        }
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut inserted: Vec<(usize, i64)> = Vec::new();
        let mut skipped = Vec::new();
        for (i, (row, allow)) in rows.iter().enumerate() {
            if !allow {
                let found: Option<i64> = tx
                    .query_row(
                        "SELECT t.id FROM transactions t LEFT JOIN voids v ON v.transaction_id = t.id \
                         WHERE v.id IS NULL AND t.kind = ?1 AND t.occurred_on = ?2 AND t.amount_cents = ?3 \
                         AND t.description = ?4 COLLATE NOCASE ORDER BY t.id LIMIT 1",
                        rusqlite::params![
                            row.kind.as_str(),
                            row.occurred_on.format("%Y-%m-%d").to_string(),
                            row.amount_cents,
                            row.description
                        ],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(matches_id) = found {
                    skipped.push(SkippedDuplicate { entry: i + 1, matches_id });
                    continue;
                }
            }
            inserted.push((i + 1, insert(&tx, row, None)?));
        }
        let batch_id = if inserted.is_empty() {
            None
        } else {
            tx.execute("INSERT INTO batches (label, recorded_at) VALUES (?1, ?2)", rusqlite::params![label, now()])?;
            let bid = tx.last_insert_rowid();
            for (_, id) in &inserted {
                tx.execute(
                    "INSERT INTO transaction_batches (transaction_id, batch_id) VALUES (?1, ?2)",
                    rusqlite::params![id, bid],
                )?;
            }
            Some(bid)
        };
        tx.commit()?;
        let mut recorded = Vec::with_capacity(inserted.len());
        for (entry, id) in inserted {
            recorded.push(RecordedEntry { entry, transaction: load(self.conn(), id)? });
        }
        Ok(BatchResult { batch_id, label, recorded, skipped_duplicates: skipped })
    }

    fn insert_one(&mut self, row: &Row) -> Result<Transaction> {
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = insert(&tx, row, None)?;
        tx.commit()?;
        load(self.conn(), id)
    }

    pub fn reclassify(&mut self, id: i64, category: &str, reason: &str) -> Result<Transaction> {
        let reason = check_text(reason, LedgerError::MissingReason)?;
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let t = load(&tx, id)?;
        if t.void.is_some() {
            return Err(LedgerError::Voided(id));
        }
        check_category(category, t.kind)?;
        tx.execute(
            "INSERT INTO classification_changes (transaction_id, category, reason, recorded_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, category, reason, now()],
        )?;
        tx.commit()?;
        load(self.conn(), id)
    }

    pub fn void(&mut self, id: i64, reason: &str) -> Result<Transaction> {
        let reason = check_text(reason, LedgerError::MissingReason)?;
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let t = load(&tx, id)?;
        if t.void.is_some() {
            return Err(LedgerError::AlreadyVoided(id));
        }
        insert_void(&tx, id, &reason)?;
        tx.commit()?;
        load(self.conn(), id)
    }

    pub fn void_batch(&mut self, batch_id: i64, reason: &str) -> Result<BatchVoid> {
        let reason = check_text(reason, LedgerError::MissingReason)?;
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let label: String = match tx.query_row("SELECT label FROM batches WHERE id = ?1", [batch_id], |r| r.get(0)) {
            Ok(l) => l,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Err(LedgerError::UnknownBatch(batch_id)),
            Err(e) => return Err(e.into()),
        };
        let members: Vec<(i64, bool)> = {
            let mut stmt = tx.prepare(
                "SELECT tb.transaction_id, v.id IS NOT NULL FROM transaction_batches tb \
                 LEFT JOIN voids v ON v.transaction_id = tb.transaction_id \
                 WHERE tb.batch_id = ?1 ORDER BY tb.transaction_id",
            )?;
            let rows = stmt.query_map([batch_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        let mut newly = Vec::new();
        let mut already_voided_ids = Vec::new();
        for (id, voided) in members {
            if voided {
                already_voided_ids.push(id);
            } else {
                insert_void(&tx, id, &reason)?;
                newly.push(id);
            }
        }
        tx.commit()?;
        let voided = newly.into_iter().map(|id| load(self.conn(), id)).collect::<Result<Vec<_>>>()?;
        Ok(BatchVoid { batch_id, label, voided, already_voided_ids })
    }

    pub fn correct(&mut self, id: i64, reason: &str, r: Replacement) -> Result<Correction> {
        let reason = check_text(reason, LedgerError::MissingReason)?;
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let orig = load(&tx, id)?;
        if orig.void.is_some() {
            return Err(LedgerError::Voided(id));
        }
        let amount_cents = r.amount_cents.unwrap_or(orig.amount_cents);
        check_amount(amount_cents)?;
        let description = check_text(
            r.description.as_deref().unwrap_or(&orig.description),
            LedgerError::MissingDescription,
        )?;
        let category = r.category.unwrap_or_else(|| orig.category.clone());
        check_category(&category, orig.kind)?;
        let source = match orig.kind {
            Kind::Expense => None,
            Kind::Income => Some(check_text(
                r.source.as_deref().or(orig.income_source.as_deref()).unwrap_or(""),
                LedgerError::MissingSource,
            )?),
        };
        let row = Row {
            kind: orig.kind,
            occurred_on: r.occurred_on.unwrap_or(orig.occurred_on),
            amount_cents,
            description,
            category,
            is_asset: orig.kind == Kind::Expense && r.is_asset.unwrap_or(orig.is_asset),
            source,
        };
        insert_void(&tx, id, &reason)?;
        let new_id = insert(&tx, &row, Some(id))?;
        tx.execute(
            "INSERT INTO transaction_batches (transaction_id, batch_id) \
             SELECT ?1, batch_id FROM transaction_batches WHERE transaction_id = ?2",
            rusqlite::params![new_id, id],
        )?;
        tx.commit()?;
        Ok(Correction { voided: load(self.conn(), id)?, replacement: load(self.conn(), new_id)? })
    }

    pub fn get(&self, id: i64) -> Result<Transaction> {
        load(self.conn(), id)
    }

    pub fn list(&self, f: &Filter) -> Result<Vec<Transaction>> {
        let mut wh: Vec<String> = Vec::new();
        let mut args: Vec<String> = Vec::new();
        let mut add = |clause: &str, v: String| {
            args.push(v);
            wh.push(clause.replace('?', &format!("?{}", args.len())));
        };
        if let Some(d) = f.from {
            add("occurred_on >= ?", d.format("%Y-%m-%d").to_string());
        }
        if let Some(d) = f.to {
            add("occurred_on <= ?", d.format("%Y-%m-%d").to_string());
        }
        if let Some(k) = f.kind {
            add("kind = ?", k.as_str().to_string());
        }
        if let Some(c) = &f.category {
            add("eff = ?", c.clone());
        }
        if let Some(s) = &f.source {
            add("src = ? COLLATE NOCASE", s.trim().to_string());
        }
        if let Some(a) = f.is_asset {
            add("is_asset = ?", (a as i64).to_string());
        }
        if let Some(b) = f.batch_id {
            add("batch_id = ?", b.to_string());
        }
        if !f.include_voided {
            wh.push("reason IS NULL".to_string());
        }
        let mut sql = format!("WITH b AS ({BASE}) SELECT * FROM b");
        if !wh.is_empty() {
            sql.push_str(&format!(" WHERE {}", wh.join(" AND ")));
        }
        sql.push_str(" ORDER BY occurred_on DESC, id DESC");
        if let Some(l) = f.limit {
            sql.push_str(&format!(" LIMIT {l}"));
        }
        let conn = self.conn();
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(args.iter()))?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            out.push(map_row(conn, r)?);
        }
        Ok(out)
    }

    pub fn income_sources(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn().prepare("SELECT name FROM income_sources ORDER BY name COLLATE NOCASE")?;
        let v = stmt.query_map([], |r| r.get(0))?.collect::<std::result::Result<Vec<String>, _>>()?;
        Ok(v)
    }
}

fn insert_void(tx: &rusqlite::Transaction, id: i64, reason: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO voids (transaction_id, reason, recorded_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![id, reason, now()],
    )?;
    Ok(())
}
