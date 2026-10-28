//! D48 to D51: the business registry and the agent's current business. Append-only like the rest of the file.
use crate::db::Ledger;
use crate::error::LedgerError;
use crate::model::*;
use chrono::SecondsFormat;
use rusqlite::{params, Connection, TransactionBehavior};

type Result<T> = std::result::Result<T, LedgerError>;

const SELECT_BIZ: &str = "WITH biz AS (
  SELECT b.id, b.created_at,
    COALESCE((SELECT c.name FROM business_changes c WHERE c.business_id = b.id AND c.action = 'rename' ORDER BY c.id DESC LIMIT 1), b.name) AS name,
    CASE WHEN EXISTS (SELECT 1 FROM business_changes c WHERE c.business_id = b.id AND c.action = 'note')
      THEN (SELECT c.note FROM business_changes c WHERE c.business_id = b.id AND c.action = 'note' ORDER BY c.id DESC LIMIT 1)
      ELSE b.note END AS note,
    COALESCE((SELECT c.is_test FROM business_changes c WHERE c.business_id = b.id AND c.action = 'test' ORDER BY c.id DESC LIMIT 1), b.is_test) AS is_test,
    (SELECT c.action FROM business_changes c WHERE c.business_id = b.id AND c.action IN ('archive','restore') ORDER BY c.id DESC LIMIT 1) AS last_state,
    (SELECT c.recorded_at FROM business_changes c WHERE c.business_id = b.id AND c.action IN ('archive','restore') ORDER BY c.id DESC LIMIT 1) AS state_at,
    COALESCE(b.id = (SELECT n.business_id FROM current_business_changes n ORDER BY n.id DESC LIMIT 1), 0) AS is_current,
    (SELECT count(*) FROM transaction_businesses t WHERE t.business_id = b.id) AS transaction_count
  FROM businesses b)
SELECT id, name, note, is_test, last_state, state_at, is_current, created_at, transaction_count FROM biz";

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn row_to_business(r: &rusqlite::Row<'_>) -> rusqlite::Result<Business> {
    let last_state: Option<String> = r.get(4)?;
    let state_at: Option<String> = r.get(5)?;
    let archived = last_state.as_deref() == Some("archive");
    Ok(Business {
        id: r.get(0)?,
        name: r.get(1)?,
        note: r.get(2)?,
        is_test: r.get::<_, i64>(3)? != 0,
        archived,
        is_current: r.get::<_, i64>(6)? != 0,
        created_at: r.get(7)?,
        archived_at: if archived { state_at } else { None },
        transaction_count: r.get(8)?,
    })
}

fn all_in(conn: &Connection) -> Result<Vec<Business>> {
    let mut st = conn.prepare(&format!("{SELECT_BIZ} ORDER BY id"))?;
    let rows = st.query_map([], row_to_business)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub(crate) fn find_in(conn: &Connection, key: &BusinessKey) -> Result<Business> {
    match key {
        BusinessKey::Id(n) => all_in(conn)?
            .into_iter()
            .find(|b| b.id == *n)
            .ok_or_else(|| LedgerError::UnknownBusiness(format!("id {n}"))),
        BusinessKey::Name(s) => {
            let t = s.trim();
            let lower = t.to_lowercase();
            let matches: Vec<Business> =
                all_in(conn)?.into_iter().filter(|b| b.name.to_lowercase() == lower).collect();
            if let Some(b) = matches.iter().find(|b| !b.archived) {
                return Ok(b.clone());
            }
            match matches.len() {
                0 => Err(LedgerError::UnknownBusiness(t.to_string())),
                1 => Ok(matches.into_iter().next().unwrap()),
                _ => Err(LedgerError::AmbiguousBusiness {
                    name: matches[0].name.clone(),
                    ids: matches.iter().map(|b| b.id.to_string()).collect::<Vec<_>>().join(", "),
                }),
            }
        }
    }
}

pub(crate) fn current_in(conn: &Connection) -> Result<Option<Business>> {
    Ok(all_in(conn)?.into_iter().find(|b| b.is_current))
}

/// The business a call works in: the named one (refused when archived), else the current one.
pub(crate) fn resolve_in(conn: &Connection, key: Option<&BusinessKey>) -> Result<Business> {
    match key {
        Some(k) => {
            let b = find_in(conn, k)?;
            if b.archived {
                return Err(LedgerError::ArchivedBusiness(b.name));
            }
            Ok(b)
        }
        None => current_in(conn)?.ok_or(LedgerError::NoCurrentBusiness),
    }
}

fn check_name(s: &str) -> Result<String> {
    let t = s.trim();
    if t.is_empty() {
        return Err(LedgerError::MissingBusinessName);
    }
    crate::guard::check_no_full_number("name", t)?;
    if t.chars().count() > 60 {
        return Err(LedgerError::BusinessNameTooLong);
    }
    Ok(t.to_string())
}

fn check_note(s: Option<&str>) -> Result<Option<String>> {
    let t = match s.map(str::trim) {
        None | Some("") => return Ok(None),
        Some(t) => t,
    };
    crate::guard::check_no_full_number("note", t)?;
    if t.chars().count() > 200 {
        return Err(LedgerError::BusinessNoteTooLong);
    }
    Ok(Some(t.to_string()))
}

fn check_free(conn: &Connection, name: &str, except: Option<i64>) -> Result<()> {
    let lower = name.to_lowercase();
    if let Some(b) = all_in(conn)?
        .into_iter()
        .find(|b| !b.archived && Some(b.id) != except && b.name.to_lowercase() == lower)
    {
        return Err(LedgerError::BusinessNameTaken(b.name));
    }
    Ok(())
}

fn set_current_row(conn: &Connection, id: Option<i64>) -> Result<()> {
    conn.execute(
        "INSERT INTO current_business_changes (business_id, recorded_at) VALUES (?1, ?2)",
        params![id, now()],
    )?;
    Ok(())
}

fn change_row(
    conn: &Connection,
    id: i64,
    action: &str,
    name: Option<&str>,
    note: Option<&str>,
    is_test: Option<bool>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO business_changes (business_id, action, name, note, is_test, recorded_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, action, name, note, is_test, now()],
    )?;
    Ok(())
}

impl Ledger {
    pub fn create_business(&mut self, b: NewBusiness) -> Result<Business> {
        let name = check_name(&b.name)?;
        let note = check_note(b.note.as_deref())?;
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        check_free(&tx, &name, None)?;
        tx.execute(
            "INSERT INTO businesses (name, note, is_test, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![name, note, b.is_test, now()],
        )?;
        let id = tx.last_insert_rowid();
        if b.make_current || current_in(&tx)?.is_none() {
            set_current_row(&tx, Some(id))?;
        }
        let out = find_in(&tx, &BusinessKey::Id(id))?;
        tx.commit()?;
        Ok(out)
    }

    /// Current first, then active by name ignoring case, then (when asked) archived by name.
    pub fn businesses(&self, include_archived: bool) -> Result<Vec<Business>> {
        let mut all: Vec<Business> =
            all_in(self.conn())?.into_iter().filter(|b| include_archived || !b.archived).collect();
        all.sort_by(|a, b| {
            b.is_current
                .cmp(&a.is_current)
                .then(a.archived.cmp(&b.archived))
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then(a.id.cmp(&b.id))
        });
        Ok(all)
    }

    pub fn find_business(&self, key: &BusinessKey) -> Result<Business> {
        find_in(self.conn(), key)
    }

    pub fn current_business(&self) -> Result<Option<Business>> {
        current_in(self.conn())
    }

    pub fn set_current_business(&mut self, key: &BusinessKey) -> Result<Business> {
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let b = resolve_in(&tx, Some(key))?;
        if !b.is_current {
            set_current_row(&tx, Some(b.id))?;
        }
        let out = find_in(&tx, &BusinessKey::Id(b.id))?;
        tx.commit()?;
        Ok(out)
    }

    pub fn rename_business(&mut self, key: &BusinessKey, name: &str) -> Result<Business> {
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = find_in(&tx, key)?;
        let new = check_name(name)?;
        check_free(&tx, &new, Some(current.id))?;
        // D45: only the identical name is a no-op; a change of letter case alone is a rename.
        if current.name == new {
            return Ok(current);
        }
        change_row(&tx, current.id, "rename", Some(&new), None, None)?;
        let out = find_in(&tx, &BusinessKey::Id(current.id))?;
        tx.commit()?;
        Ok(out)
    }

    pub fn set_business_note(&mut self, key: &BusinessKey, note: Option<&str>) -> Result<Business> {
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = find_in(&tx, key)?;
        let new = check_note(note)?;
        if current.note == new {
            return Ok(current);
        }
        change_row(&tx, current.id, "note", None, new.as_deref(), None)?;
        let out = find_in(&tx, &BusinessKey::Id(current.id))?;
        tx.commit()?;
        Ok(out)
    }

    pub fn set_business_test(&mut self, key: &BusinessKey, is_test: bool) -> Result<Business> {
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = find_in(&tx, key)?;
        if current.is_test == is_test {
            return Ok(current);
        }
        change_row(&tx, current.id, "test", None, None, Some(is_test))?;
        let out = find_in(&tx, &BusinessKey::Id(current.id))?;
        tx.commit()?;
        Ok(out)
    }

    pub fn archive_business(&mut self, key: &BusinessKey) -> Result<Business> {
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = find_in(&tx, key)?;
        if current.archived {
            return Err(LedgerError::AlreadyArchived(current.name));
        }
        change_row(&tx, current.id, "archive", None, None, None)?;
        if current.is_current {
            set_current_row(&tx, None)?;
        }
        let out = find_in(&tx, &BusinessKey::Id(current.id))?;
        tx.commit()?;
        Ok(out)
    }

    pub fn restore_business(&mut self, key: &BusinessKey) -> Result<Business> {
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = find_in(&tx, key)?;
        if !current.archived {
            return Err(LedgerError::NotArchived(current.name));
        }
        check_free(&tx, &current.name, Some(current.id))?;
        change_row(&tx, current.id, "restore", None, None, None)?;
        let out = find_in(&tx, &BusinessKey::Id(current.id))?;
        tx.commit()?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_in_follows_current_and_refuses_archived() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut l = Ledger::open(&dir.path().join("l.sqlite3")).unwrap();
        assert_eq!(resolve_in(l.conn(), None).unwrap().id, 1);
        l.create_business(NewBusiness { name: "Shop".into(), note: None, is_test: false, make_current: true })
            .unwrap();
        assert_eq!(resolve_in(l.conn(), None).unwrap().name, "Shop");
        assert_eq!(resolve_in(l.conn(), Some(&BusinessKey::Id(1))).unwrap().id, 1);
        l.archive_business(&BusinessKey::Id(2)).unwrap();
        assert!(matches!(resolve_in(l.conn(), None), Err(LedgerError::NoCurrentBusiness)));
        assert!(matches!(
            resolve_in(l.conn(), Some(&BusinessKey::Id(2))),
            Err(LedgerError::ArchivedBusiness(n)) if n == "Shop"
        ));
    }
}
