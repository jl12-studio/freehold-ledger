//! D28 to D35: the payment method registry. Append-only like the rest of the file.
use crate::db::Ledger;
use crate::error::LedgerError;
use crate::model::*;
use chrono::SecondsFormat;
use rusqlite::{params, Connection, TransactionBehavior};

type Result<T> = std::result::Result<T, LedgerError>;

const SELECT_PM: &str = "WITH pm AS (
  SELECT p.id, p.kind, p.last4, p.created_at,
    COALESCE((SELECT c.nickname FROM payment_method_changes c WHERE c.payment_method_id = p.id AND c.action = 'rename' ORDER BY c.id DESC LIMIT 1), p.nickname) AS nickname,
    (SELECT c.recorded_at FROM payment_method_changes c WHERE c.payment_method_id = p.id AND c.action = 'retire' ORDER BY c.id LIMIT 1) AS retired_at,
    COALESCE(p.id = (SELECT d.payment_method_id FROM business_default_payment_method_changes d WHERE d.business_id = pb.business_id ORDER BY d.id DESC LIMIT 1), 0) AS is_default,
    pb.business_id AS business_id,
    (SELECT count(*) FROM transactions t LEFT JOIN voids v ON v.transaction_id = t.id
      WHERE v.id IS NULL AND (SELECT a.payment_method_id FROM payment_method_assignments a WHERE a.transaction_id = t.id ORDER BY a.id DESC LIMIT 1) = p.id) AS transaction_count
  FROM payment_methods p JOIN payment_method_businesses pb ON pb.payment_method_id = p.id)
SELECT id, kind, nickname, last4, is_default, retired_at, created_at, transaction_count, business_id FROM pm";

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn row_to_pm(r: &rusqlite::Row<'_>) -> rusqlite::Result<PaymentMethod> {
    let kind: String = r.get(1)?;
    let retired_at: Option<String> = r.get(5)?;
    Ok(PaymentMethod {
        id: r.get(0)?,
        kind: PaymentKind::from_db(&kind),
        nickname: r.get(2)?,
        last4: r.get(3)?,
        is_default: r.get::<_, i64>(4)? != 0,
        retired: retired_at.is_some(),
        created_at: r.get(6)?,
        retired_at,
        transaction_count: r.get(7)?,
        business_id: r.get(8)?,
    })
}

fn all_in(conn: &Connection, business_id: i64) -> Result<Vec<PaymentMethod>> {
    let mut st = conn.prepare(&format!("{SELECT_PM} WHERE business_id = ?1 ORDER BY id"))?;
    let rows = st.query_map([business_id], row_to_pm)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn by_id(conn: &Connection, id: i64) -> Result<PaymentMethod> {
    let mut st = conn.prepare(&format!("{SELECT_PM} WHERE id = ?1"))?;
    let mut rows = st.query_map([id], row_to_pm)?.collect::<rusqlite::Result<Vec<_>>>()?;
    rows.pop().ok_or_else(|| LedgerError::UnknownPaymentMethod(format!("id {id}")))
}

pub(crate) fn find_in(conn: &Connection, key: &PaymentMethodKey, business_id: i64) -> Result<PaymentMethod> {
    match key {
        PaymentMethodKey::Id(n) => {
            let p = by_id(conn, *n)?;
            if p.business_id != business_id {
                let business = crate::businesses::find_in(conn, &BusinessKey::Id(p.business_id))?.name;
                return Err(LedgerError::PaymentMethodInOtherBusiness { nickname: p.nickname, business });
            }
            Ok(p)
        }
        PaymentMethodKey::Nickname(s) => {
            let t = s.trim();
            let lower = t.to_lowercase();
            let matches: Vec<PaymentMethod> =
                all_in(conn, business_id)?.into_iter().filter(|p| p.nickname.to_lowercase() == lower).collect();
            if let Some(p) = matches.iter().find(|p| !p.retired) {
                return Ok(p.clone());
            }
            match matches.len() {
                0 => Err(LedgerError::UnknownPaymentMethod(t.to_string())),
                1 => Ok(matches.into_iter().next().unwrap()),
                _ => Err(LedgerError::AmbiguousPaymentMethod {
                    nickname: matches[0].nickname.clone(),
                    ids: matches.iter().map(|p| p.id.to_string()).collect::<Vec<_>>().join(", "),
                }),
            }
        }
    }
}

/// One method as a transaction shows it, by id. A single-row lookup: `find_in` loads every
/// method with its transaction count, which is far too slow to run once per listed row.
pub(crate) fn ref_in(conn: &Connection, id: i64) -> Result<PaymentMethodRef> {
    conn.query_row(
        "SELECT p.id, p.kind, \
           COALESCE((SELECT c.nickname FROM payment_method_changes c WHERE c.payment_method_id = p.id AND c.action = 'rename' ORDER BY c.id DESC LIMIT 1), p.nickname), \
           p.last4, \
           EXISTS (SELECT 1 FROM payment_method_changes c WHERE c.payment_method_id = p.id AND c.action = 'retire') \
         FROM payment_methods p WHERE p.id = ?1",
        [id],
        |r| {
            let kind: String = r.get(1)?;
            Ok(PaymentMethodRef {
                id: r.get(0)?,
                kind: PaymentKind::from_db(&kind),
                nickname: r.get(2)?,
                last4: r.get(3)?,
                retired: r.get::<_, i64>(4)? != 0,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => LedgerError::UnknownPaymentMethod(format!("id {id}")),
        other => other.into(),
    })
}

pub(crate) fn active_in(conn: &Connection, key: &PaymentMethodKey, business_id: i64) -> Result<PaymentMethod> {
    let p = find_in(conn, key, business_id)?;
    if p.retired {
        return Err(LedgerError::RetiredPaymentMethod(p.nickname));
    }
    Ok(p)
}

pub(crate) fn default_in(conn: &Connection, business_id: i64) -> Result<Option<PaymentMethod>> {
    Ok(all_in(conn, business_id)?.into_iter().find(|p| p.is_default))
}

fn check_nickname(s: &str) -> Result<String> {
    let t = s.trim();
    if t.is_empty() {
        return Err(LedgerError::MissingNickname);
    }
    crate::guard::check_no_full_number("nickname", t)?;
    if t.chars().count() > 40 {
        return Err(LedgerError::NicknameTooLong);
    }
    if t.eq_ignore_ascii_case("none") {
        return Err(LedgerError::NicknameReserved);
    }
    Ok(t.to_string())
}

fn check_free(conn: &Connection, nickname: &str, except: Option<i64>, business_id: i64) -> Result<()> {
    let lower = nickname.to_lowercase();
    if let Some(p) = all_in(conn, business_id)?
        .into_iter()
        .find(|p| !p.retired && Some(p.id) != except && p.nickname.to_lowercase() == lower)
    {
        return Err(LedgerError::NicknameTaken(p.nickname));
    }
    Ok(())
}

fn set_default_row(conn: &Connection, business_id: i64, id: Option<i64>) -> Result<()> {
    conn.execute(
        "INSERT INTO business_default_payment_method_changes (business_id, payment_method_id, recorded_at) VALUES (?1, ?2, ?3)",
        params![business_id, id, now()],
    )?;
    Ok(())
}

/// The business a read works in: the named one (archived allowed), else the current one.
fn read_business(conn: &Connection, key: Option<&BusinessKey>) -> Result<i64> {
    Ok(match key {
        Some(k) => crate::businesses::find_in(conn, k)?.id,
        None => crate::businesses::current_in(conn)?.ok_or(LedgerError::NoCurrentBusiness)?.id,
    })
}

impl Ledger {
    pub fn create_payment_method(&mut self, p: NewPaymentMethod) -> Result<PaymentMethod> {
        self.create_payment_method_in(None, p)
    }

    pub fn create_payment_method_in(
        &mut self,
        business: Option<&BusinessKey>,
        p: NewPaymentMethod,
    ) -> Result<PaymentMethod> {
        let nickname = check_nickname(&p.nickname)?;
        let last4 = match &p.last4 {
            None => None,
            Some(raw) => {
                crate::guard::check_no_full_number("last4", raw)?;
                let t = raw.trim();
                if t.len() != 4 || !t.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(LedgerError::InvalidLast4);
                }
                if p.kind == PaymentKind::Cash {
                    return Err(LedgerError::Last4NotForCash);
                }
                Some(t.to_string())
            }
        };
        if p.kind == PaymentKind::Cash && p.last4.is_some() {
            return Err(LedgerError::Last4NotForCash);
        }
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let biz = crate::businesses::resolve_in(&tx, business)?;
        check_free(&tx, &nickname, None, biz.id)?;
        tx.execute(
            "INSERT INTO payment_methods (kind, nickname, last4, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![p.kind.as_str(), nickname, last4, now()],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO payment_method_businesses (payment_method_id, business_id) VALUES (?1, ?2)",
            params![id, biz.id],
        )?;
        if p.make_default {
            set_default_row(&tx, biz.id, Some(id))?;
        }
        let out = find_in(&tx, &PaymentMethodKey::Id(id), biz.id)?;
        tx.commit()?;
        Ok(out)
    }

    pub fn payment_methods(&self, include_retired: bool) -> Result<Vec<PaymentMethod>> {
        self.payment_methods_in(None, include_retired)
    }

    pub fn payment_methods_in(
        &self,
        business: Option<&BusinessKey>,
        include_retired: bool,
    ) -> Result<Vec<PaymentMethod>> {
        let biz = read_business(self.conn(), business)?;
        let mut all: Vec<PaymentMethod> =
            all_in(self.conn(), biz)?.into_iter().filter(|p| include_retired || !p.retired).collect();
        all.sort_by(|a, b| {
            b.is_default
                .cmp(&a.is_default)
                .then(a.retired.cmp(&b.retired))
                .then(a.nickname.to_lowercase().cmp(&b.nickname.to_lowercase()))
                .then(a.id.cmp(&b.id))
        });
        Ok(all)
    }

    pub fn find_payment_method(&self, key: &PaymentMethodKey) -> Result<PaymentMethod> {
        self.find_payment_method_in(None, key)
    }

    pub fn find_payment_method_in(
        &self,
        business: Option<&BusinessKey>,
        key: &PaymentMethodKey,
    ) -> Result<PaymentMethod> {
        let biz = read_business(self.conn(), business)?;
        find_in(self.conn(), key, biz)
    }

    pub fn default_payment_method(&self) -> Result<Option<PaymentMethod>> {
        self.default_payment_method_in(None)
    }

    pub fn default_payment_method_in(&self, business: Option<&BusinessKey>) -> Result<Option<PaymentMethod>> {
        let biz = read_business(self.conn(), business)?;
        default_in(self.conn(), biz)
    }

    pub fn rename_payment_method(&mut self, key: &PaymentMethodKey, nickname: &str) -> Result<PaymentMethod> {
        self.rename_payment_method_in(None, key, nickname)
    }

    pub fn rename_payment_method_in(
        &mut self,
        business: Option<&BusinessKey>,
        key: &PaymentMethodKey,
        nickname: &str,
    ) -> Result<PaymentMethod> {
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let biz = crate::businesses::resolve_in(&tx, business)?;
        let current = active_in(&tx, key, biz.id)?;
        let new = check_nickname(nickname)?;
        check_free(&tx, &new, Some(current.id), biz.id)?;
        // D45: only the identical nickname is a no-op; a change of letter case alone is a rename.
        if current.nickname == new {
            return Ok(current);
        }
        tx.execute(
            "INSERT INTO payment_method_changes (payment_method_id, action, nickname, recorded_at) VALUES (?1, 'rename', ?2, ?3)",
            params![current.id, new, now()],
        )?;
        let out = find_in(&tx, &PaymentMethodKey::Id(current.id), biz.id)?;
        tx.commit()?;
        Ok(out)
    }

    pub fn retire_payment_method(&mut self, key: &PaymentMethodKey) -> Result<PaymentMethod> {
        self.retire_payment_method_in(None, key)
    }

    pub fn retire_payment_method_in(
        &mut self,
        business: Option<&BusinessKey>,
        key: &PaymentMethodKey,
    ) -> Result<PaymentMethod> {
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let biz = crate::businesses::resolve_in(&tx, business)?;
        let current = find_in(&tx, key, biz.id)?;
        if current.retired {
            return Err(LedgerError::AlreadyRetired(current.nickname));
        }
        tx.execute(
            "INSERT INTO payment_method_changes (payment_method_id, action, nickname, recorded_at) VALUES (?1, 'retire', NULL, ?2)",
            params![current.id, now()],
        )?;
        if current.is_default {
            set_default_row(&tx, biz.id, None)?;
        }
        let out = find_in(&tx, &PaymentMethodKey::Id(current.id), biz.id)?;
        tx.commit()?;
        Ok(out)
    }

    pub fn set_default_payment_method(
        &mut self,
        key: Option<&PaymentMethodKey>,
    ) -> Result<Option<PaymentMethod>> {
        self.set_default_payment_method_in(None, key)
    }

    pub fn set_default_payment_method_in(
        &mut self,
        business: Option<&BusinessKey>,
        key: Option<&PaymentMethodKey>,
    ) -> Result<Option<PaymentMethod>> {
        let tx = self.conn_mut().transaction_with_behavior(TransactionBehavior::Immediate)?;
        let biz = crate::businesses::resolve_in(&tx, business)?;
        let out = match key {
            Some(k) => {
                let p = active_in(&tx, k, biz.id)?;
                if !p.is_default {
                    set_default_row(&tx, biz.id, Some(p.id))?;
                }
                Some(find_in(&tx, &PaymentMethodKey::Id(p.id), biz.id)?)
            }
            None => {
                if default_in(&tx, biz.id)?.is_some() {
                    set_default_row(&tx, biz.id, None)?;
                }
                None
            }
        };
        tx.commit()?;
        Ok(out)
    }
}
