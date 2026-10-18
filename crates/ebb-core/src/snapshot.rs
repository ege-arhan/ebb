//! Tarama anlık görüntüleri ve "neden büyüdü" karşılaştırması.
use crate::{Result, journal::Journal, plan::now, scan::Node};
use rusqlite::params;
use serde::Serialize;
use std::collections::BTreeMap;

pub type Sizes = BTreeMap<String, u64>;

#[derive(Serialize, Debug)]
pub struct Growth {
    pub path: String,
    pub before: u64,
    pub after: u64,
    pub delta: i64,
}

/// Klasörlerin (en çok `depth` seviye) boyut haritası.
pub fn flatten(node: &Node, depth: usize) -> Sizes {
    fn go(n: &Node, d: usize, out: &mut Sizes) {
        if !n.is_dir {
            return;
        }
        out.insert(n.path.to_string_lossy().into_owned(), n.size);
        if d > 0 {
            for c in &n.children {
                go(c, d - 1, out);
            }
        }
    }
    let mut out = Sizes::new();
    go(node, depth, &mut out);
    out
}

impl Journal {
    pub fn save_snapshot(&self, root: &str, sizes: &Sizes) -> Result<()> {
        self.conn.execute(
            "INSERT INTO snapshots(root,ts,body) VALUES(?1,?2,?3)",
            params![root, now(), serde_json::to_string(sizes)?],
        )?;
        Ok(())
    }

    pub fn latest_snapshot(&self, root: &str) -> Result<Option<(i64, Sizes)>> {
        let row = self
            .conn
            .query_row(
                "SELECT ts,body FROM snapshots WHERE root=?1 ORDER BY ts DESC LIMIT 1",
                [root],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            )
            .ok();
        row.map(|(ts, b)| Ok((ts, serde_json::from_str(&b)?)))
            .transpose()
    }
}

/// En çok büyüyen klasörler (azalan delta). Eski anlık görüntüde olmayan klasör 0'dan başlar.
pub fn growth(before: &Sizes, after: &Sizes, top: usize) -> Vec<Growth> {
    let mut v: Vec<Growth> = after
        .iter()
        .map(|(p, &a)| {
            let b = before.get(p).copied().unwrap_or(0);
            Growth {
                path: p.clone(),
                before: b,
                after: a,
                delta: a as i64 - b as i64,
            }
        })
        .filter(|g| g.delta > 0)
        .collect();
    v.sort_by_key(|a| std::cmp::Reverse(a.delta));
    v.truncate(top);
    v
}
