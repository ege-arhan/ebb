//! Temizlik kataloğu: YAML tarifleri ikiliye gömülür, kullanıcı ek klasör verebilir.
use crate::{Result, scan::measure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Labels {
    pub en: String,
    #[serde(default)]
    pub tr: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Safety {
    /// Kendini yeniden oluşturan önbellek.
    Safe,
    /// Kullanıcı görmeden silinmez.
    Review,
    /// Hiçbir zaman plana girmez.
    Never,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Paths {
    #[serde(default)]
    pub all: Vec<String>,
    #[serde(default)]
    pub macos: Vec<String>,
    #[serde(default)]
    pub windows: Vec<String>,
    #[serde(default)]
    pub linux: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CatalogEntry {
    pub id: String,
    pub label: Labels,
    pub category: String,
    pub safety: Safety,
    #[serde(default)]
    pub min_size_mb: u64,
    pub paths: Paths,
    #[serde(default)]
    pub note: Option<Labels>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Finding {
    pub entry_id: String,
    pub label: String,
    pub category: String,
    pub safety: Safety,
    pub path: PathBuf,
    pub size: u64,
    pub files: u64,
    /// Kullanıcıya neden dikkat etmesi gerektiğini anlatan kısa not (varsa).
    #[serde(default)]
    pub note: Option<String>,
}

const BUILTIN: &[&str] = &[
    include_str!("../../../catalog/browsers.yaml"),
    include_str!("../../../catalog/dev.yaml"),
    include_str!("../../../catalog/apps.yaml"),
    include_str!("../../../catalog/system.yaml"),
    include_str!("../../../catalog/ai.yaml"),
];

pub fn builtin() -> Vec<CatalogEntry> {
    BUILTIN
        .iter()
        .flat_map(|y| {
            serde_yaml::from_str::<Vec<CatalogEntry>>(y)
                .expect("embedded catalog must be valid YAML")
        })
        .collect()
}

/// Yerleşik katalog + `dir` altındaki `*.yaml` tarifleri (aynı id kullanıcıyı kazandırır).
pub fn load(extra_dir: Option<&Path>) -> Result<Vec<CatalogEntry>> {
    let mut all = builtin();
    if let Some(dir) = extra_dir.filter(|d| d.is_dir()) {
        for e in std::fs::read_dir(dir)?.flatten() {
            if e.path()
                .extension()
                .is_some_and(|x| x == "yaml" || x == "yml")
            {
                let extra: Vec<CatalogEntry> =
                    serde_yaml::from_str(&std::fs::read_to_string(e.path())?)?;
                for x in extra {
                    all.retain(|a| a.id != x.id);
                    all.push(x);
                }
            }
        }
    }
    Ok(all)
}

fn expand(raw: &str) -> String {
    let rep = |tok: &str, p: Option<PathBuf>| {
        p.map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| tok.into())
    };
    raw.replace("{home}", &rep("{home}", dirs::home_dir()))
        .replace("{cache}", &rep("{cache}", dirs::cache_dir()))
        .replace("{data}", &rep("{data}", dirs::data_dir()))
        .replace("{local}", &rep("{local}", dirs::data_local_dir()))
        .replace("{temp}", &std::env::temp_dir().to_string_lossy())
}

/// Girdinin bu işletim sistemindeki, gerçekten var olan yolları.
pub fn resolve(entry: &CatalogEntry) -> Vec<PathBuf> {
    let os = match std::env::consts::OS {
        "macos" => &entry.paths.macos,
        "windows" => &entry.paths.windows,
        _ => &entry.paths.linux,
    };
    let mut out = vec![];
    for raw in entry.paths.all.iter().chain(os.iter()) {
        let pat = expand(raw);
        if pat.contains('{') {
            continue; // genişletilemeyen belirteç: bu OS'te anlamsız
        }
        if let Ok(paths) = glob::glob(&pat) {
            out.extend(paths.flatten());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Kataloğu makinede çalıştırır: var olan ve eşik üstü yolları boyutlarıyla döner (büyükten küçüğe).
pub fn findings(entries: &[CatalogEntry], lang_tr: bool) -> Vec<Finding> {
    let jobs: Vec<(&CatalogEntry, PathBuf)> = entries
        .iter()
        .flat_map(|e| resolve(e).into_iter().map(move |p| (e, p)))
        .collect();
    let mut found: Vec<Finding> = std::thread::scope(|s| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|(e, p)| {
                s.spawn(move || {
                    let (size, files) = measure(p);
                    (size >= e.min_size_mb * 1_000_000 && size > 0).then(|| Finding {
                        entry_id: e.id.clone(),
                        label: if lang_tr {
                            e.label.tr.clone().unwrap_or_else(|| e.label.en.clone())
                        } else {
                            e.label.en.clone()
                        },
                        category: e.category.clone(),
                        safety: e.safety,
                        path: p.clone(),
                        size,
                        files,
                        note: e.note.as_ref().map(|n| {
                            if lang_tr {
                                n.tr.clone().unwrap_or_else(|| n.en.clone())
                            } else {
                                n.en.clone()
                            }
                        }),
                    })
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().ok().flatten())
            .collect()
    });
    found.sort_by_key(|a| std::cmp::Reverse(a.size));
    found
}
