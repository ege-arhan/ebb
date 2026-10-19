//! Auto Care: zamanlanmış kurallar. Kural plan HAZIRLAR; yalnızca güvenli türlerde ve kullanıcı `auto_apply`
//! açtıysa uygular. Uygulanan her plan normal günlük planıdır (Journal'da görünür, geri alınabilir).
use crate::{
    Result, catalog,
    finders::{self, FoundFile},
    guard,
    journal::Journal,
    plan::{Plan, PlanItem, Select, now},
    scan::measure,
    trashbin::{self, TrashEntry},
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, SystemTime},
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub kind: RuleKind,
    pub schedule: Schedule,
    pub enabled: bool,
    pub auto_apply: bool,
    pub last_run: Option<i64>,
    pub last_result: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleKind {
    SafeCaches,
    OldInstallers {
        min_age_days: u64,
    },
    TrashOlderThan {
        days: u64,
    },
    /// Asla otomatik uygulanmaz.
    LargeOld {
        min_mb: u64,
        min_age_days: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Schedule {
    Daily,
    /// 0 = Pazartesi (UTC).
    Weekly {
        weekday: u8,
    },
    OnLowDisk {
        free_percent: u8,
    },
}

#[derive(Serialize, Debug)]
pub struct RuleOutcome {
    pub rule_id: String,
    pub plan_id: Option<String>,
    pub reclaimable: u64,
    pub applied: bool,
    /// Başarıyla taşınan (ya da çöpten silinen) öğe sayısı.
    pub moved: u64,
    pub message: String,
}

impl Journal {
    fn ensure_rules(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS rules(id TEXT PRIMARY KEY, body TEXT NOT NULL);",
        )?;
        Ok(())
    }

    pub fn rules(&self) -> Result<Vec<Rule>> {
        self.ensure_rules()?;
        let mut st = self.conn.prepare("SELECT body FROM rules ORDER BY rowid")?;
        let bodies: Vec<String> = st.query_map([], |r| r.get(0))?.flatten().collect();
        // Bozuk bir satır diğer kuralları engellemesin.
        Ok(bodies
            .iter()
            .filter_map(|b| serde_json::from_str(b).ok())
            .collect())
    }

    pub fn save_rule(&self, r: &Rule) -> Result<()> {
        // Kalıcı silme sınırı: Çöp'te 7 günden kısa bekleyenler otomatik silinemez; alt sınırlar zorlanır.
        let mut r = r.clone();
        match &mut r.kind {
            RuleKind::TrashOlderThan { days } => *days = (*days).max(7),
            RuleKind::LargeOld {
                min_mb,
                min_age_days,
            } => {
                *min_mb = (*min_mb).max(50);
                *min_age_days = (*min_age_days).max(30);
            }
            RuleKind::SafeCaches | RuleKind::OldInstallers { .. } => {}
        }
        let r = &r;
        self.ensure_rules()?;
        self.conn.execute(
            "INSERT INTO rules(id,body) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET body=excluded.body",
            params![r.id, serde_json::to_string(r)?],
        )?;
        Ok(())
    }

    pub fn delete_rule(&self, id: &str) -> Result<()> {
        self.ensure_rules()?;
        self.conn.execute("DELETE FROM rules WHERE id=?1", [id])?;
        Ok(())
    }

    /// Yalnızca günlükte kayıtlı kuralın last_run/last_result alanlarını günceller (kaydı yoktan yaratmaz).
    fn mark_run(&self, rule: &Rule, at: i64, result: &str) -> Result<()> {
        if let Some(mut r) = self.rules()?.into_iter().find(|r| r.id == rule.id) {
            r.last_run = Some(at);
            r.last_result = Some(result.into());
            self.save_rule(&r)?;
        }
        Ok(())
    }
}

/// Mantıklı varsayılanlar; hepsi kapalı gelir.
pub fn default_rules() -> Vec<Rule> {
    let mk = |id: &str, name: &str, kind, schedule| Rule {
        id: id.into(),
        name: name.into(),
        kind,
        schedule,
        enabled: false,
        auto_apply: false,
        last_run: None,
        last_result: None,
    };
    vec![
        mk(
            "weekly-safe-caches",
            "Weekly safe cache cleanup",
            RuleKind::SafeCaches,
            Schedule::Weekly { weekday: 6 },
        ),
        mk(
            "old-installers-30d",
            "Installers older than 30 days",
            RuleKind::OldInstallers { min_age_days: 30 },
            Schedule::Weekly { weekday: 6 },
        ),
        mk(
            "trash-30d",
            "Empty Trash items older than 30 days",
            RuleKind::TrashOlderThan { days: 30 },
            Schedule::Weekly { weekday: 6 },
        ),
        mk(
            "low-disk-caches",
            "Clean safe caches when disk is low",
            RuleKind::SafeCaches,
            Schedule::OnLowDisk { free_percent: 10 },
        ),
    ]
}

/// Kural şimdi çalışmalı mı? Saf fonksiyon. Hafta günü `now`'ın UTC günüdür.
pub fn is_due(rule: &Rule, now: i64, free_percent: f32) -> bool {
    if !rule.enabled {
        return false;
    }
    let since = |secs: i64| rule.last_run.is_none_or(|l| now - l >= secs);
    match rule.schedule {
        Schedule::Daily => since(20 * 3600),
        Schedule::Weekly { weekday } => {
            // 1970-01-01 Perşembe (Pzt=0 → 3).
            let wd = (now.div_euclid(86_400) + 3).rem_euclid(7);
            wd == i64::from(weekday) && since(6 * 86_400)
        }
        Schedule::OnLowDisk { free_percent: t } => free_percent <= f32::from(t) && since(6 * 3600),
    }
}

/// `auto_apply` yalnızca güvenli türlerde geçerlidir.
pub fn effective_auto_apply(rule: &Rule) -> bool {
    rule.auto_apply && !matches!(rule.kind, RuleKind::LargeOld { .. })
}

const INSTALLER_EXT: &[&str] = &[
    "dmg", "pkg", "mpkg", "iso", "exe", "msi", "msix", "appimage", "deb", "rpm", "xip",
];

/// `dirs` içindeki (özyinelemesiz), `min_age_days` günden eski kurulum dosyaları. Symlink'ler ve korunan yollar atlanır.
pub fn installers_in(dirs: &[PathBuf], min_age_days: u64) -> Vec<PlanItem> {
    let now = SystemTime::now();
    let mut out = vec![];
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let is_installer = p
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| INSTALLER_EXT.contains(&x.to_ascii_lowercase().as_str()));
            let Ok(m) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if !is_installer || m.file_type().is_symlink() || guard::check(&p).is_err() {
                continue;
            }
            let t = m
                .accessed()
                .ok()
                .into_iter()
                .chain(m.modified().ok())
                .max()
                .unwrap_or(now);
            let age = now.duration_since(t).unwrap_or(Duration::ZERO).as_secs() / 86_400;
            if age >= min_age_days {
                out.push(PlanItem {
                    size: if m.is_dir() { measure(&p).0 } else { m.len() },
                    path: p,
                    entry_id: None,
                    reason: format!("installer (untouched {age} days)"),
                });
            }
        }
    }
    out.sort_by_key(|i| std::cmp::Reverse(i.size));
    out
}

fn found_item(f: FoundFile) -> PlanItem {
    PlanItem {
        path: f.path,
        size: f.size,
        entry_id: None,
        reason: format!("{} (untouched {} days)", f.kind, f.age_days),
    }
}

pub fn large_old_in(root: &Path, min_mb: u64, min_age_days: u64) -> Result<Vec<PlanItem>> {
    let files = finders::large_old_files(
        root,
        min_mb.saturating_mul(1_000_000),
        min_age_days,
        200,
        &AtomicBool::new(false),
    )?;
    Ok(files.into_iter().map(found_item).collect())
}

/// Çöpte `days` günden uzun süredir duran öğeler (tarihi bilinmeyen atlanır).
pub fn old_trash(entries: &[TrashEntry], days: u64, now: i64) -> Vec<&TrashEntry> {
    let secs = i64::try_from(days.saturating_mul(86_400)).unwrap_or(i64::MAX);
    entries
        .iter()
        .filter(|e| e.deleted_at.is_some_and(|t| now - t >= secs))
        .collect()
}

fn outcome(rule: &Rule, message: String) -> RuleOutcome {
    RuleOutcome {
        rule_id: rule.id.clone(),
        plan_id: None,
        reclaimable: 0,
        applied: false,
        moved: 0,
        message,
    }
}

/// Planı günlüğe yazar, izin varsa uygular.
fn save_and_maybe_apply(journal: &Journal, rule: &Rule, plan: Plan) -> Result<RuleOutcome> {
    let mut out = outcome(rule, String::new());
    if plan.items.is_empty() {
        out.message = "Nothing to clean".into();
        return Ok(out);
    }
    journal.save_plan(&plan)?;
    out.plan_id = Some(plan.id.clone());
    out.reclaimable = plan.total;
    if effective_auto_apply(rule) {
        let rep = journal.apply(&plan.id)?;
        out.applied = true;
        out.moved = (rep.items.len() - rep.failed) as u64;
        out.message = format!(
            "Moved {} item(s) to the Trash ({} failed)",
            out.moved, rep.failed
        );
    } else {
        out.message = format!(
            "Plan ready: {} item(s), review it in Ebb to apply",
            plan.items.len()
        );
    }
    Ok(out)
}

fn execute(journal: &Journal, rule: &Rule) -> Result<RuleOutcome> {
    match &rule.kind {
        RuleKind::SafeCaches => {
            let found = catalog::findings(&catalog::load(None)?, false);
            save_and_maybe_apply(journal, rule, Plan::from_findings(&found, &Select::Safe))
        }
        RuleKind::OldInstallers { min_age_days } => {
            let dirs: Vec<PathBuf> = [dirs::download_dir(), dirs::desktop_dir()]
                .into_iter()
                .flatten()
                .collect();
            let items = installers_in(&dirs, *min_age_days);
            save_and_maybe_apply(journal, rule, Plan::new(items))
        }
        RuleKind::LargeOld {
            min_mb,
            min_age_days,
        } => {
            let items = match dirs::home_dir() {
                Some(h) => large_old_in(&h, *min_mb, *min_age_days)?,
                None => vec![],
            };
            save_and_maybe_apply(journal, rule, Plan::new(items))
        }
        RuleKind::TrashOlderThan { days } => {
            let all = trashbin::list(journal)?;
            let old = old_trash(&all, *days, now());
            let mut out = outcome(rule, String::new());
            out.reclaimable = old.iter().map(|e| e.size).sum();
            if old.is_empty() {
                out.message = "Nothing to clean".into();
            } else if effective_auto_apply(rule) {
                let ids: Vec<String> = old.iter().map(|e| e.id.clone()).collect();
                trashbin::purge(journal, &ids)?;
                out.applied = true;
                out.moved = ids.len() as u64;
                out.message = format!("Permanently deleted {} old Trash item(s)", ids.len());
            } else {
                // Kalıcı silme: izin yoksa yalnızca boyut bildirilir.
                out.message = format!(
                    "{} old Trash item(s) could be emptied; enable auto-apply or empty the Trash yourself",
                    old.len()
                );
            }
            Ok(out)
        }
    }
}

/// Kuralı şimdi çalıştırır; last_run/last_result günlükte kayıtlı kurala yazılır.
pub fn run_rule(journal: &Journal, rule: &Rule) -> Result<RuleOutcome> {
    let out = execute(journal, rule)?;
    journal.mark_run(rule, now(), &out.message)?;
    Ok(out)
}

/// Arka plan döngüsü için: zamanı gelen her kuralı çalıştırır. Bir kuralın hatası diğerlerini durdurmaz.
pub fn run_due(journal: &Journal, now_ts: i64, free_percent: f32) -> Vec<RuleOutcome> {
    let rules = journal.rules().unwrap_or_default();
    rules
        .iter()
        .filter(|r| is_due(r, now_ts, free_percent))
        .map(|r| {
            run_rule(journal, r).unwrap_or_else(|e| {
                let msg = format!("Failed: {e}");
                // Hata da last_run'ı ilerletir: her döngüde yeniden denenmesin.
                let _ = journal.mark_run(r, now_ts, &msg);
                outcome(r, msg)
            })
        })
        .collect()
}
