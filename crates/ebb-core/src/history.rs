//! Boş alan geçmişi: saatte en fazla bir örnek, 120 gün saklanır. "Diskim zamanla nasıl doldu?" grafiği için.
use crate::{Result, journal::Journal, plan::now};
use rusqlite::params;
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct DiskSample {
    pub ts: i64,
    pub total: u64,
    pub free: u64,
}

impl Journal {
    fn ensure_history(&self) -> Result<()> {
        self.conn.execute_batch("CREATE TABLE IF NOT EXISTS disk_history(ts INTEGER PRIMARY KEY, total INTEGER, free INTEGER);")?;
        Ok(())
    }

    /// Son örnek bir saatten eskiyse kaydeder; 120 günden eskileri siler.
    pub fn record_disk(&self, total: u64, free: u64) -> Result<bool> {
        self.ensure_history()?;
        let t = now();
        let last: Option<i64> =
            self.conn
                .query_row("SELECT MAX(ts) FROM disk_history", [], |r| r.get(0))?;
        if last.is_some_and(|l| t - l < 3600) {
            return Ok(false);
        }
        self.conn.execute(
            "INSERT OR REPLACE INTO disk_history(ts,total,free) VALUES(?1,?2,?3)",
            params![t, total as i64, free as i64],
        )?;
        self.conn
            .execute("DELETE FROM disk_history WHERE ts < ?1", [t - 120 * 86_400])?;
        Ok(true)
    }

    pub fn disk_history(&self, days: u32) -> Result<Vec<DiskSample>> {
        self.ensure_history()?;
        let mut st = self
            .conn
            .prepare("SELECT ts,total,free FROM disk_history WHERE ts >= ?1 ORDER BY ts")?;
        let rows = st.query_map([now() - days as i64 * 86_400], |r| {
            Ok(DiskSample {
                ts: r.get(0)?,
                total: r.get::<_, i64>(1)? as u64,
                free: r.get::<_, i64>(2)? as u64,
            })
        })?;
        Ok(rows.flatten().collect())
    }
}
