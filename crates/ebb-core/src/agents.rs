//! AI araçlarının ve yerel modellerin disk kullanımı.
use crate::scan::measure;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize, Debug, Clone)]
pub struct AiStore {
    pub id: &'static str,
    pub label: &'static str,
    /// "agent" (kod ajanı verisi) ya da "model" (model ağırlıkları/önbellek).
    pub kind: &'static str,
    pub path: PathBuf,
    pub size: u64,
    pub files: u64,
}

fn known() -> Vec<(&'static str, &'static str, &'static str, PathBuf)> {
    let h = dirs::home_dir().unwrap_or_default();
    let ollama = std::env::var_os("OLLAMA_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| h.join(".ollama/models"));
    vec![
        ("claude-code", "Claude Code", "agent", h.join(".claude")),
        ("codex", "Codex", "agent", h.join(".codex")),
        ("cursor", "Cursor", "agent", h.join(".cursor")),
        ("gemini-cli", "Gemini CLI", "agent", h.join(".gemini")),
        (
            "windsurf",
            "Windsurf / Codeium",
            "agent",
            h.join(".codeium"),
        ),
        ("ollama", "Ollama models", "model", ollama),
        (
            "lm-studio",
            "LM Studio models",
            "model",
            h.join(".lmstudio/models"),
        ),
        (
            "lm-studio-cache",
            "LM Studio cache",
            "model",
            h.join(".cache/lm-studio"),
        ),
        (
            "huggingface",
            "Hugging Face cache",
            "model",
            h.join(".cache/huggingface"),
        ),
        ("torch", "PyTorch cache", "model", h.join(".cache/torch")),
    ]
}

/// Var olan AI depolarını boyutlarıyla döner (büyükten küçüğe).
pub fn ai_storage() -> Vec<AiStore> {
    let mut v: Vec<AiStore> = std::thread::scope(|s| {
        let hs: Vec<_> = known()
            .into_iter()
            .filter(|(_, _, _, p)| p.exists())
            .map(|(id, label, kind, path)| {
                s.spawn(move || {
                    let (size, files) = measure(&path);
                    AiStore {
                        id,
                        label,
                        kind,
                        path,
                        size,
                        files,
                    }
                })
            })
            .collect();
        hs.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    v.sort_by_key(|a| std::cmp::Reverse(a.size));
    v
}
