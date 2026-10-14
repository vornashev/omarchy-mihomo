//! The user's routing rules and their change journal.
//!
//! Every change is journaled with the rule before and after it. Rolling back
//! replays inverse changes, which are journaled too, so a rollback can itself
//! be rolled back.

use std::path::Path;

use omm_config::rules::{MatchKind, UserRule, sort_by_specificity};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

pub type Result<T, E = rusqlite::Error> = std::result::Result<T, E>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stored {
    #[serde(flatten)]
    pub rule: UserRule,
    #[serde(default)]
    pub note: Option<String>,
    /// `user`, or where an imported rule came from (`site-autotune`, ...).
    pub source: String,
    /// Unix milliseconds.
    pub created: i64,
    pub updated: i64,
    /// Latest route check before saving.
    #[serde(default)]
    pub test: Option<Check>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub target: String,
    pub ok: bool,
    #[serde(default)]
    pub delay: Option<u32>,
    #[serde(default)]
    pub error: Option<String>,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Add,
    Update,
    Delete,
    /// Inverse of an earlier change (`reverts`).
    Revert,
}

#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub id: i64,
    pub at: i64,
    pub action: Action,
    pub rule_id: i64,
    pub before: Option<Stored>,
    pub after: Option<Stored>,
    /// For `revert`: the change undone.
    pub reverts: Option<i64>,
}

/// Fields for a new rule.
pub struct NewRule {
    pub kind: MatchKind,
    /// Already normalized.
    pub value: String,
    pub target: String,
    pub note: Option<String>,
    pub source: String,
    pub test: Option<Check>,
}

/// Partial update; `None` leaves a field as is.
#[derive(Debug, Default, Deserialize)]
pub struct Patch {
    pub target: Option<String>,
    pub enabled: Option<bool>,
    pub note: Option<String>,
    pub test: Option<Check>,
}

pub struct Store {
    db: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::with(Connection::open(path)?)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        Self::with(Connection::open_in_memory()?)
    }

    fn with(db: Connection) -> Result<Self> {
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 2000;
             CREATE TABLE IF NOT EXISTS user_rules (
                 id INTEGER PRIMARY KEY,
                 kind TEXT NOT NULL,
                 value TEXT NOT NULL,
                 data TEXT NOT NULL,
                 UNIQUE (kind, value)
             );
             CREATE TABLE IF NOT EXISTS rule_changes (
                 id INTEGER PRIMARY KEY,
                 at INTEGER NOT NULL,
                 action TEXT NOT NULL,
                 rule_id INTEGER NOT NULL,
                 before TEXT,
                 after TEXT,
                 reverts INTEGER
             );",
        )?;
        Ok(Self { db })
    }

    /// All rules in the order they are rendered (most specific first).
    pub fn all(&self) -> Result<Vec<Stored>> {
        let mut stmt = self.db.prepare_cached("SELECT data FROM user_rules")?;
        let mut rules: Vec<Stored> = stmt.query_map([], |r| r.get::<_, String>(0))?.filter_map(|d| d.ok()).filter_map(|d| serde_json::from_str(&d).ok()).collect();
        let mut plain: Vec<UserRule> = rules.iter().map(|s| s.rule.clone()).collect();
        sort_by_specificity(&mut plain);
        let order: std::collections::HashMap<i64, usize> = plain.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
        rules.sort_by_key(|s| order[&s.rule.id]);
        Ok(rules)
    }

    pub fn rules(&self) -> Result<Vec<UserRule>> {
        Ok(self.all()?.into_iter().map(|s| s.rule).collect())
    }

    pub fn get(&self, id: i64) -> Result<Option<Stored>> {
        self.db
            .query_row("SELECT data FROM user_rules WHERE id = ?1", [id], |r| r.get::<_, String>(0))
            .optional()
            .map(|d| d.and_then(|d| serde_json::from_str(&d).ok()))
    }

    pub fn find(&self, kind: MatchKind, value: &str) -> Result<Option<Stored>> {
        let id: Option<i64> = self.db.query_row("SELECT id FROM user_rules WHERE kind = ?1 AND value = ?2", params![kind_str(kind), value], |r| r.get(0)).optional()?;
        id.map_or(Ok(None), |id| self.get(id))
    }

    /// Add a rule, or retarget the existing rule for the same match.
    pub fn upsert(&mut self, new: NewRule, now: i64) -> Result<(Stored, Change)> {
        if let Some(existing) = self.find(new.kind, &new.value)? {
            let patch = Patch { target: Some(new.target), enabled: Some(true), note: new.note, test: new.test };
            return self.update(existing.rule.id, patch, now).map(|o| o.expect("row exists"));
        }
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO user_rules (kind, value, data) VALUES (?1, ?2, '{}')", params![kind_str(new.kind), new.value])?;
        let id = tx.last_insert_rowid();
        let stored = Stored {
            rule: UserRule { id, kind: new.kind, value: new.value, target: new.target, enabled: true },
            note: new.note,
            source: new.source,
            created: now,
            updated: now,
            test: new.test,
        };
        tx.execute("UPDATE user_rules SET data = ?2 WHERE id = ?1", params![id, json(&stored)])?;
        let change = journal(&tx, now, Action::Add, id, None, Some(&stored), None)?;
        tx.commit()?;
        Ok((stored, change))
    }

    pub fn update(&mut self, id: i64, patch: Patch, now: i64) -> Result<Option<(Stored, Change)>> {
        let Some(before) = self.get(id)? else { return Ok(None) };
        let mut after = before.clone();
        if let Some(t) = patch.target {
            after.rule.target = t;
        }
        if let Some(e) = patch.enabled {
            after.rule.enabled = e;
        }
        if let Some(n) = patch.note {
            after.note = Some(n).filter(|n| !n.trim().is_empty());
        }
        if let Some(t) = patch.test {
            after.test = Some(t);
        }
        after.updated = now;
        let tx = self.db.transaction()?;
        tx.execute("UPDATE user_rules SET data = ?2 WHERE id = ?1", params![id, json(&after)])?;
        let change = journal(&tx, now, Action::Update, id, Some(&before), Some(&after), None)?;
        tx.commit()?;
        Ok(Some((after, change)))
    }

    pub fn delete(&mut self, id: i64, now: i64) -> Result<Option<Change>> {
        let Some(before) = self.get(id)? else { return Ok(None) };
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM user_rules WHERE id = ?1", [id])?;
        let change = journal(&tx, now, Action::Delete, id, Some(&before), None, None)?;
        tx.commit()?;
        Ok(Some(change))
    }

    /// Newest first.
    pub fn history(&self, limit: usize) -> Result<Vec<Change>> {
        let mut stmt = self.db.prepare("SELECT id, at, action, rule_id, before, after, reverts FROM rule_changes ORDER BY id DESC LIMIT ?1")?;
        stmt.query_map([limit as i64], change_row)?.collect()
    }

    /// Drop changes `from..` as if they never happened: restore the rules
    /// and delete the journal entries. Only for changes that never reached
    /// the core (a failed apply), so the journal keeps describing reality.
    pub fn discard_since(&mut self, from: i64) -> Result<()> {
        let pending: Vec<Change> = {
            let mut stmt = self.db.prepare("SELECT id, at, action, rule_id, before, after, reverts FROM rule_changes WHERE id >= ?1 ORDER BY id DESC")?;
            stmt.query_map([from], change_row)?.collect::<Result<_>>()?
        };
        let tx = self.db.transaction()?;
        for c in &pending {
            restore(&tx, c)?;
        }
        tx.execute("DELETE FROM rule_changes WHERE id >= ?1", [from])?;
        tx.commit()
    }

    /// Undo every change made after `change_id` (exclusive), newest first.
    /// `change_id = 0` undoes everything. Returns the inverse changes.
    pub fn rollback_to(&mut self, change_id: i64, now: i64) -> Result<Vec<Change>> {
        let pending: Vec<Change> = {
            let mut stmt = self.db.prepare("SELECT id, at, action, rule_id, before, after, reverts FROM rule_changes WHERE id > ?1 ORDER BY id DESC")?;
            stmt.query_map([change_id], change_row)?.collect::<Result<_>>()?
        };
        let tx = self.db.transaction()?;
        let mut done = Vec::new();
        for c in pending {
            // Restore the state before `c`: `before` comes back, `after` goes.
            let current: Option<Stored> = tx
                .query_row("SELECT data FROM user_rules WHERE id = ?1", [c.rule_id], |r| r.get::<_, String>(0))
                .optional()?
                .and_then(|d| serde_json::from_str(&d).ok());
            restore(&tx, &c)?;
            done.push(journal(&tx, now, Action::Revert, c.rule_id, current.as_ref(), c.before.as_ref(), Some(c.id))?);
        }
        tx.commit()?;
        Ok(done)
    }
}

/// Put a rule back to its state before `c`.
fn restore(tx: &Connection, c: &Change) -> Result<()> {
    match &c.before {
        Some(b) => {
            // The match may be taken by a newer rule; drop that one first.
            tx.execute("DELETE FROM user_rules WHERE kind = ?1 AND value = ?2 AND id != ?3", params![kind_str(b.rule.kind), b.rule.value, b.rule.id])?;
            tx.execute(
                "INSERT INTO user_rules (id, kind, value, data) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(id) DO UPDATE SET kind = ?2, value = ?3, data = ?4",
                params![b.rule.id, kind_str(b.rule.kind), b.rule.value, json(b)],
            )?;
        }
        None => {
            tx.execute("DELETE FROM user_rules WHERE id = ?1", [c.rule_id])?;
        }
    }
    Ok(())
}

fn journal(db: &Connection, at: i64, action: Action, rule_id: i64, before: Option<&Stored>, after: Option<&Stored>, reverts: Option<i64>) -> Result<Change> {
    let act = serde_json::to_value(&action).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default();
    db.execute(
        "INSERT INTO rule_changes (at, action, rule_id, before, after, reverts) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![at, act, rule_id, before.map(json), after.map(json), reverts],
    )?;
    Ok(Change { id: db.last_insert_rowid(), at, action, rule_id, before: before.cloned(), after: after.cloned(), reverts })
}

fn change_row(r: &Row) -> Result<Change> {
    let parse = |s: Option<String>| s.and_then(|s| serde_json::from_str::<Stored>(&s).ok());
    let action: String = r.get(2)?;
    Ok(Change {
        id: r.get(0)?,
        at: r.get(1)?,
        action: serde_json::from_value(serde_json::Value::from(action)).unwrap_or(Action::Update),
        rule_id: r.get(3)?,
        before: parse(r.get(4)?),
        after: parse(r.get(5)?),
        reverts: r.get(6)?,
    })
}

fn json(s: &Stored) -> String {
    serde_json::to_string(s).expect("rule serializes")
}

fn kind_str(k: MatchKind) -> &'static str {
    match k {
        MatchKind::Domain => "domain",
        MatchKind::DomainSuffix => "domain-suffix",
        MatchKind::IpCidr => "ip-cidr",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(kind: MatchKind, value: &str, target: &str) -> NewRule {
        NewRule { kind, value: value.into(), target: target.into(), note: None, source: "user".into(), test: None }
    }

    fn summary(s: &Store) -> Vec<(String, String, bool)> {
        s.all().unwrap().into_iter().map(|r| (r.rule.value, r.rule.target, r.rule.enabled)).collect()
    }

    #[test]
    fn upsert_retargets_same_match() {
        let mut s = Store::in_memory().unwrap();
        let (a, c1) = s.upsert(new(MatchKind::DomainSuffix, "x.com", "AI"), 1).unwrap();
        assert_eq!(c1.action, Action::Add);
        let (b, c2) = s.upsert(new(MatchKind::DomainSuffix, "x.com", "PROXY"), 2).unwrap();
        assert_eq!(a.rule.id, b.rule.id);
        assert_eq!(c2.action, Action::Update);
        assert_eq!(c2.before.unwrap().rule.target, "AI");
        assert_eq!(summary(&s), [("x.com".into(), "PROXY".into(), true)]);
        // Same value, different kind, is a different rule.
        s.upsert(new(MatchKind::Domain, "x.com", "DIRECT"), 3).unwrap();
        assert_eq!(s.all().unwrap().len(), 2);
        assert_eq!(s.all().unwrap()[0].rule.kind, MatchKind::Domain, "exact before suffix");
    }

    #[test]
    fn rollback_replays_inverse_changes() {
        let mut s = Store::in_memory().unwrap();
        let (a, _) = s.upsert(new(MatchKind::DomainSuffix, "a.com", "AI"), 1).unwrap();
        let checkpoint = s.history(1).unwrap()[0].id;
        s.upsert(new(MatchKind::DomainSuffix, "b.com", "AI"), 2).unwrap();
        s.update(a.rule.id, Patch { enabled: Some(false), ..Default::default() }, 3).unwrap();
        s.delete(a.rule.id, 4).unwrap();
        assert_eq!(summary(&s), [("b.com".into(), "AI".into(), true)]);

        let undone = s.rollback_to(checkpoint, 5).unwrap();
        assert_eq!(undone.len(), 3);
        assert!(undone.iter().all(|c| c.action == Action::Revert));
        assert_eq!(summary(&s), [("a.com".into(), "AI".into(), true)]);
        assert_eq!(s.all().unwrap()[0].rule.id, a.rule.id, "deleted rule comes back with its id");

        // Rolling back the rollback restores the later state.
        let before_rollback = undone.last().unwrap().id - 3;
        s.rollback_to(before_rollback, 6).unwrap();
        assert_eq!(summary(&s), [("b.com".into(), "AI".into(), true)]);
    }

    #[test]
    fn rollback_everything_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.db");
        {
            let mut s = Store::open(&path).unwrap();
            s.upsert(new(MatchKind::IpCidr, "1.2.3.4/32", "DIRECT"), 1).unwrap();
            s.upsert(new(MatchKind::Domain, "x.com", "AI"), 2).unwrap();
        }
        let mut s = Store::open(&path).unwrap();
        assert_eq!(s.all().unwrap().len(), 2);
        assert_eq!(s.history(10).unwrap().len(), 2);
        s.rollback_to(0, 3).unwrap();
        assert!(s.all().unwrap().is_empty());
        assert_eq!(s.history(10).unwrap()[0].reverts, Some(1));
    }

    #[test]
    fn discarded_changes_leave_no_trace() {
        let mut s = Store::in_memory().unwrap();
        s.upsert(new(MatchKind::Domain, "keep.com", "AI"), 1).unwrap();
        let (_, c) = s.upsert(new(MatchKind::Domain, "keep.com", "Nope"), 2).unwrap();
        s.upsert(new(MatchKind::Domain, "new.com", "Nope"), 3).unwrap();
        s.discard_since(c.id).unwrap();
        assert_eq!(summary(&s), [("keep.com".into(), "AI".into(), true)]);
        assert_eq!(s.history(10).unwrap().len(), 1);
    }

    #[test]
    fn missing_rows_are_none() {
        let mut s = Store::in_memory().unwrap();
        assert!(s.update(42, Patch::default(), 1).unwrap().is_none());
        assert!(s.delete(42, 1).unwrap().is_none());
    }
}
