//! Plan: yıkıcı her işlem önce bir plandır. Plan uygulanmadan hiçbir şey silinmez.
use crate::{
    catalog::{Finding, Safety},
    guard,
    scan::measure,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlanItem {
    pub path: PathBuf,
    pub size: u64,
    pub entry_id: Option<String>,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Plan {
    pub id: String,
    pub created: i64,
    pub items: Vec<PlanItem>,
    pub total: u64,
    /// Koruma listesine takılıp plana alınmayanlar: (yol, neden).
    pub skipped: Vec<(PathBuf, String)>,
}

pub enum Select {
    /// Yalnızca `safe` bulgular.
    Safe,
    /// Verilen katalog id'leri (safe ve review). `never` her zaman dışarıda.
    Ids(Vec<String>),
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Plan {
    pub fn new(candidates: Vec<PlanItem>) -> Plan {
        let created = now();
        let (mut items, mut skipped) = (vec![], vec![]);
        for it in candidates {
            match guard::check(&it.path) {
                Ok(()) => items.push(it),
                Err(e) => skipped.push((it.path, e.to_string())),
            }
        }
        let total = items.iter().map(|i| i.size).sum();
        let mut h = blake3::Hasher::new();
        h.update(&created.to_le_bytes());
        for i in &items {
            h.update(i.path.to_string_lossy().as_bytes());
        }
        Plan {
            id: h.finalize().to_hex()[..12].to_string(),
            created,
            items,
            total,
            skipped,
        }
    }

    pub fn from_findings(findings: &[Finding], select: &Select) -> Plan {
        let items = findings
            .iter()
            .filter(|f| match select {
                Select::Safe => f.safety == Safety::Safe,
                Select::Ids(ids) => f.safety != Safety::Never && ids.contains(&f.entry_id),
            })
            .map(|f| PlanItem {
                path: f.path.clone(),
                size: f.size,
                entry_id: Some(f.entry_id.clone()),
                reason: f.label.clone(),
            })
            .collect();
        Plan::new(items)
    }

    pub fn from_paths(paths: &[PathBuf]) -> Plan {
        // Önce koruma listesi: `/` ya da ev klasörü gibi yolların boyutu hiç ölçülmez.
        let (mut ok, mut blocked) = (vec![], vec![]);
        for p in paths {
            match guard::check(p) {
                Ok(()) => ok.push(PlanItem {
                    path: p.clone(),
                    size: measure(p).0,
                    entry_id: None,
                    reason: "selected by you".into(),
                }),
                Err(e) => blocked.push((p.clone(), e.to_string())),
            }
        }
        let mut plan = Plan::new(ok);
        plan.skipped.extend(blocked);
        plan
    }
}
