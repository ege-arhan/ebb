//! Çöp Kutusu: içeriği listele, Ebb'in taşıdıklarını geri yükle, seçilenleri kalıcı sil, tamamını boşalt.
//! Kalıcı silme her zaman kullanıcının açık eylemiyle olur; MCP bunu sunmaz.
use crate::{Error, Result, journal::Journal, scan::measure};
use rusqlite::params;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
pub struct TrashEntry {
    /// Geri yükleme/silme için kimlik (macOS'ta çöpteki tam yol, diğerlerinde sistemin kimliği).
    pub id: String,
    pub name: String,
    pub size: u64,
    pub deleted_at: Option<i64>,
    /// Bilinen özgün konum (Ebb taşıdıysa ya da sistem kaydediyorsa).
    pub original: Option<PathBuf>,
    pub by_ebb: bool,
    pub plan_id: Option<String>,
    pub restorable: bool,
}

#[derive(Serialize, Debug, Default)]
pub struct TrashSummary {
    pub items: usize,
    pub size: u64,
}

impl Journal {
    fn entry_by(&self, column: &str, value: &str) -> Option<(i64, String, String)> {
        self.conn
            .query_row(
                &format!("SELECT id, original, plan_id FROM entries WHERE {column}=?1 AND status='applied' ORDER BY id DESC LIMIT 1"),
                [value],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok()
    }

    /// Girdinin durumunu değiştirir; planda "applied" öğe kalmadıysa planı da günceller.
    fn settle(&self, entry_id: i64, plan_id: &str, status: &str) {
        let _ = self.conn.execute(
            "UPDATE entries SET status=?2 WHERE id=?1",
            params![entry_id, status],
        );
        let left: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE plan_id=?1 AND status='applied'",
                [plan_id],
                |r| r.get(0),
            )
            .unwrap_or(1);
        if left == 0 {
            let plan_status = if status == "purged" {
                "purged"
            } else {
                "undone"
            };
            let _ = self.conn.execute(
                "UPDATE plans SET status=?2 WHERE id=?1",
                params![plan_id, plan_status],
            );
        }
    }
}

fn sum(entries: &[TrashEntry]) -> TrashSummary {
    TrashSummary {
        items: entries.len(),
        size: entries.iter().map(|e| e.size).sum(),
    }
}

pub fn summary(journal: &Journal) -> Result<TrashSummary> {
    list(journal).map(|l| sum(&l))
}

// ---------------- macOS ----------------
#[cfg(target_os = "macos")]
fn trash_dir() -> Result<PathBuf> {
    dirs::home_dir()
        .map(|h| h.join(".Trash"))
        .ok_or_else(|| Error::NotFound("home folder".into()))
}

#[cfg(target_os = "macos")]
fn denied(e: std::io::Error) -> Error {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        Error::Guard("macOS blocked access to the Trash. Give Ebb Full Disk Access in System Settings → Privacy & Security, then try again".into())
    } else {
        e.into()
    }
}

#[cfg(target_os = "macos")]
pub fn list(journal: &Journal) -> Result<Vec<TrashEntry>> {
    use std::os::unix::fs::MetadataExt;
    let dir = trash_dir()?;
    let paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(denied)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n != ".DS_Store" && n != ".localized")
        })
        .collect();
    // Boyutlar paralel ölçülür: büyük bir çöpte bile ekran hızlı açılsın.
    let sizes: Vec<u64> = std::thread::scope(|s| {
        let hs: Vec<_> = paths
            .iter()
            .map(|p| s.spawn(move || measure(p).0))
            .collect();
        hs.into_iter().map(|h| h.join().unwrap_or(0)).collect()
    });
    let mut out: Vec<TrashEntry> = paths
        .into_iter()
        .zip(sizes)
        .map(|(p, size)| {
            let id = p.to_string_lossy().into_owned();
            let ebb = journal.entry_by("trashed", &id);
            let by_ebb = ebb.is_some();
            TrashEntry {
                name: p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                size,
                deleted_at: std::fs::symlink_metadata(&p).ok().map(|m| m.ctime()),
                original: ebb.as_ref().map(|e| PathBuf::from(&e.1)),
                by_ebb,
                plan_id: ebb.map(|e| e.2),
                restorable: by_ebb,
                id,
            }
        })
        .collect();
    out.sort_by_key(|e| std::cmp::Reverse(e.deleted_at.unwrap_or(0)));
    Ok(out)
}

/// Kimliğin gerçekten ~/.Trash'in doğrudan bir öğesi olduğunu doğrular (başka bir yeri silmeye kapı açmasın).
#[cfg(target_os = "macos")]
fn in_trash(id: &str) -> Result<PathBuf> {
    let dir = trash_dir()?.canonicalize().map_err(denied)?;
    let p = PathBuf::from(id);
    let parent = p.parent().and_then(|x| x.canonicalize().ok());
    if parent.as_deref() != Some(dir.as_path())
        || p.file_name().is_none()
        || p.symlink_metadata().is_err()
    {
        return Err(Error::Guard(format!("{id} is not an item in the Trash")));
    }
    Ok(p)
}

#[cfg(target_os = "macos")]
pub fn restore(journal: &Journal, ids: &[String]) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        let p = in_trash(id)?;
        let (eid, original, plan) = journal.entry_by("trashed", id).ok_or_else(|| {
            Error::Guard(format!(
                "Ebb doesn't know where {} came from. Use Finder's Put Back for it",
                p.display()
            ))
        })?;
        crate::platform::restore_item(Path::new(&original), Some(&p))?;
        journal.settle(eid, &plan, "undone");
        n += 1;
    }
    Ok(n)
}

#[cfg(target_os = "macos")]
pub fn purge(journal: &Journal, ids: &[String]) -> Result<u64> {
    let mut bytes = 0;
    for id in ids {
        let p = in_trash(id)?;
        bytes += measure(&p).0;
        let m = p.symlink_metadata()?;
        if m.is_dir() {
            std::fs::remove_dir_all(&p)
        } else {
            std::fs::remove_file(&p)
        }
        .map_err(denied)?;
        if let Some((eid, _, plan)) = journal.entry_by("trashed", id) {
            journal.settle(eid, &plan, "purged");
        }
    }
    Ok(bytes)
}

/// Finder'ın kendi "Çöp Kutusunu Boşalt" komutu: tüm disklerdeki çöpü boşaltır.
#[cfg(target_os = "macos")]
pub fn empty(journal: &Journal) -> Result<u64> {
    let items = list(journal)?;
    let before = sum(&items).size;
    let ok = std::process::Command::new("osascript")
        .args(["-e", "tell application \"Finder\" to empty trash"])
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        purge(
            journal,
            &items.iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
        )?;
    }
    for e in items.iter().filter(|e| e.by_ebb) {
        if let Some((eid, _, plan)) = journal.entry_by("trashed", &e.id) {
            journal.settle(eid, &plan, "purged");
        }
    }
    Ok(before)
}

// ---------------- Windows / Linux ----------------
#[cfg(all(
    not(target_os = "macos"),
    any(
        target_os = "windows",
        all(unix, not(target_os = "ios"), not(target_os = "android"))
    )
))]
mod sys {
    use super::*;
    use trash::os_limited;

    fn tr(e: trash::Error) -> Error {
        Error::Trash(e.to_string())
    }

    fn size_of(item: &trash::TrashItem) -> u64 {
        if let Some(s) = os_limited::metadata(item).ok().and_then(|m| m.size.size()) {
            return s;
        }
        // Klasörlerde sistem yalnızca öğe sayısı verir; dosyaların gerçek yerini ölç.
        let id = PathBuf::from(&item.id);
        if cfg!(windows) {
            return measure(&id).0;
        }
        // Linux: .../Trash/info/X.trashinfo → .../Trash/files/X
        id.parent()
            .and_then(|info| info.parent())
            .map(|base| base.join("files").join(&item.name))
            .map(|p| measure(&p).0)
            .unwrap_or(0)
    }

    pub fn list(journal: &Journal) -> Result<Vec<TrashEntry>> {
        let mut out: Vec<TrashEntry> = os_limited::list()
            .map_err(tr)?
            .into_iter()
            .map(|i| {
                let original = i.original_path();
                let ebb = journal.entry_by("original", &original.to_string_lossy());
                TrashEntry {
                    id: i.id.to_string_lossy().into_owned(),
                    name: i.name.to_string_lossy().into_owned(),
                    size: size_of(&i),
                    deleted_at: Some(i.time_deleted),
                    original: Some(original),
                    by_ebb: ebb.is_some(),
                    plan_id: ebb.map(|e| e.2),
                    restorable: true,
                }
            })
            .collect();
        out.sort_by_key(|e| std::cmp::Reverse(e.deleted_at.unwrap_or(0)));
        Ok(out)
    }

    fn pick(ids: &[String]) -> Result<Vec<trash::TrashItem>> {
        let all = os_limited::list().map_err(tr)?;
        let found: Vec<_> = all
            .into_iter()
            .filter(|i| ids.iter().any(|x| *x == i.id.to_string_lossy()))
            .collect();
        if found.len() != ids.len() {
            return Err(Error::NotFound(
                "some items are no longer in the Trash".into(),
            ));
        }
        Ok(found)
    }

    fn settle_all(journal: &Journal, items: &[trash::TrashItem], status: &str) {
        for i in items {
            if let Some((eid, _, plan)) =
                journal.entry_by("original", &i.original_path().to_string_lossy())
            {
                journal.settle(eid, &plan, status);
            }
        }
    }

    pub fn restore(journal: &Journal, ids: &[String]) -> Result<usize> {
        let items = pick(ids)?;
        let n = items.len();
        settle_all(journal, &items, "undone");
        os_limited::restore_all(items).map_err(tr)?;
        Ok(n)
    }

    pub fn purge(journal: &Journal, ids: &[String]) -> Result<u64> {
        let items = pick(ids)?;
        let bytes = items.iter().map(size_of).sum();
        settle_all(journal, &items, "purged");
        os_limited::purge_all(items).map_err(tr)?;
        Ok(bytes)
    }

    pub fn empty(journal: &Journal) -> Result<u64> {
        let items = os_limited::list().map_err(tr)?;
        let bytes = items.iter().map(size_of).sum();
        settle_all(journal, &items, "purged");
        os_limited::purge_all(items).map_err(tr)?;
        Ok(bytes)
    }
}

#[cfg(all(
    not(target_os = "macos"),
    any(
        target_os = "windows",
        all(unix, not(target_os = "ios"), not(target_os = "android"))
    )
))]
pub use sys::{empty, list, purge, restore};
