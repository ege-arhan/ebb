//! SQLite işlem günlüğü: planlar, uygulanan öğeler, geri alma, tarama anlık görüntüleri.
use crate::{
    Error, Result, guard,
    plan::{Plan, now},
    platform,
};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub struct Journal {
    pub(crate) conn: Connection,
}

#[derive(Serialize, Debug)]
pub struct ItemResult {
    pub path: PathBuf,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct Report {
    pub plan_id: String,
    pub items: Vec<ItemResult>,
    /// Çöpe taşınan toplam bayt (Çöp boşaltılınca yer açılır).
    pub bytes: u64,
    pub failed: usize,
}

#[derive(Serialize, Debug)]
pub struct EntryRow {
    pub original: String,
    pub size: u64,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct PlanSummary {
    pub id: String,
    pub created: i64,
    pub total: u64,
    pub status: String,
}

impl Journal {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS plans(id TEXT PRIMARY KEY, created INTEGER, total INTEGER, status TEXT, body TEXT);
             CREATE TABLE IF NOT EXISTS entries(id INTEGER PRIMARY KEY AUTOINCREMENT, plan_id TEXT, original TEXT, trashed TEXT,
               size INTEGER, status TEXT, error TEXT, at INTEGER);
             CREATE TABLE IF NOT EXISTS snapshots(root TEXT, ts INTEGER, body TEXT);",
        )?;
        Ok(Self { conn })
    }

    pub fn open_default() -> Result<Self> {
        Self::open(&crate::db_path())
    }

    pub fn save_plan(&self, p: &Plan) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO plans(id,created,total,status,body) VALUES(?1,?2,?3,'planned',?4)",
            params![p.id, p.created, p.total as i64, serde_json::to_string(p)?],
        )?;
        Ok(())
    }

    pub fn plan(&self, id: &str) -> Result<(Plan, String)> {
        self.conn
            .query_row("SELECT body,status FROM plans WHERE id=?1", [id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|_| Error::NotFound(format!("plan {id}")))
            .and_then(|(b, s)| Ok((serde_json::from_str(&b)?, s)))
    }

    pub fn list(&self, limit: usize) -> Result<Vec<PlanSummary>> {
        let mut st = self
            .conn
            .prepare("SELECT id,created,total,status FROM plans ORDER BY created DESC LIMIT ?1")?;
        let rows = st.query_map([limit as i64], |r| {
            Ok(PlanSummary {
                id: r.get(0)?,
                created: r.get(1)?,
                total: r.get::<_, i64>(2)? as u64,
                status: r.get(3)?,
            })
        })?;
        Ok(rows.flatten().collect())
    }

    /// Bir planın uygulanan öğeleri (günlük ayrıntısı için).
    pub fn items(&self, id: &str) -> Result<Vec<EntryRow>> {
        let mut st = self.conn.prepare(
            "SELECT original,size,status,error FROM entries WHERE plan_id=?1 ORDER BY id",
        )?;
        let rows = st.query_map([id], |r| {
            Ok(EntryRow {
                original: r.get(0)?,
                size: r.get::<_, i64>(1)? as u64,
                status: r.get(2)?,
                error: r.get(3)?,
            })
        })?;
        Ok(rows.flatten().collect())
    }

    /// Planı uygular: her öğe önce koruma listesinden geçer, sonra Çöp Kutusu'na taşınır.
    pub fn apply(&self, id: &str) -> Result<Report> {
        let (plan, status) = self.plan(id)?;
        if status != "planned" {
            return Err(Error::Guard(format!(
                "plan {id} is '{status}' and cannot be applied again"
            )));
        }
        let mut report = Report {
            plan_id: id.into(),
            items: vec![],
            bytes: 0,
            failed: 0,
        };
        for it in &plan.items {
            // Günlüğe, taşımadan ÖNCE çözümlenmiş (symlink'siz) yol yazılır: geri alırken zincir değiştiyse fark edilir.
            let canon = it.path.canonicalize().unwrap_or_else(|_| it.path.clone());
            let res = guard::check(&it.path).and_then(|()| platform::trash_item(&it.path));
            let (status, trashed, err) = match &res {
                Ok(t) => (
                    "applied",
                    t.as_ref().map(|p| p.to_string_lossy().into_owned()),
                    None,
                ),
                Err(e) => ("failed", None, Some(e.to_string())),
            };
            self.conn.execute(
                "INSERT INTO entries(plan_id,original,trashed,size,status,error,at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![id, canon.to_string_lossy(), trashed, it.size as i64, status, err, now()],
            )?;
            if res.is_ok() {
                report.bytes += it.size;
            } else {
                report.failed += 1;
            }
            report.items.push(ItemResult {
                path: it.path.clone(),
                ok: res.is_ok(),
                error: err,
            });
        }
        self.conn.execute(
            "UPDATE plans SET status=?2 WHERE id=?1",
            params![
                id,
                if report.failed == plan.items.len() && !plan.items.is_empty() {
                    "failed"
                } else {
                    "applied"
                }
            ],
        )?;
        Ok(report)
    }

    /// Planın uygulanan öğelerini özgün yerine geri koyar.
    pub fn undo(&self, id: &str) -> Result<Report> {
        let mut st = self.conn.prepare(
            "SELECT id,original,trashed,size FROM entries WHERE plan_id=?1 AND status='applied'",
        )?;
        let rows: Vec<(i64, String, Option<String>, i64)> = st
            .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .flatten()
            .collect();
        if rows.is_empty() {
            return Err(Error::NotFound(format!("nothing to undo for plan {id}")));
        }
        let mut report = Report {
            plan_id: id.into(),
            items: vec![],
            bytes: 0,
            failed: 0,
        };
        for (eid, original, trashed, size) in rows {
            // Önce durum güncellenir (işlem içinde), sonra dosya taşınır: DB yazımı başarısızsa dosya yerinde kalır,
            // taşıma başarısızsa işlem geri alınır. İkisi birbirinden kopuk kalmaz.
            let tx = self.conn.unchecked_transaction()?;
            tx.execute("UPDATE entries SET status='undone' WHERE id=?1", [eid])?;
            let res =
                platform::restore_item(Path::new(&original), trashed.as_deref().map(Path::new));
            if res.is_ok() {
                tx.commit()?;
                report.bytes += size as u64;
            } else {
                drop(tx);
                report.failed += 1;
            }
            report.items.push(ItemResult {
                path: original.into(),
                ok: res.is_ok(),
                error: res.err().map(|e| e.to_string()),
            });
        }
        let left: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM entries WHERE plan_id=?1 AND status='applied'",
            [id],
            |r| r.get(0),
        )?;
        if left == 0 {
            self.conn
                .execute("UPDATE plans SET status='undone' WHERE id=?1", [id])?;
        }
        Ok(report)
    }

    /// `days` günden eski kayıtları siler (geri alma penceresi, varsayılan 90).
    pub fn prune(&self, days: i64) -> Result<usize> {
        let cut = now() - days * 86_400;
        self.conn
            .execute("DELETE FROM entries WHERE at < ?1", [cut])?;
        Ok(self
            .conn
            .execute("DELETE FROM plans WHERE created < ?1", [cut])?)
    }
}
