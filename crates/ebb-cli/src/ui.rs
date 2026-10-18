//! Terminal görünümü: renk, çubuk, dalga, biçimlendirme. NO_COLOR ve boru hattında (pipe) renksiz çalışır (`console`).
use console::{Color, Style, StyledObject, style};
use std::path::Path;

pub fn human(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let (mut v, mut i) = (b as f64, 0);
    while v >= 1000.0 && i < U.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    match i {
        0 => format!("{b} B"),
        _ if v >= 100.0 => format!("{v:.0} {}", U[i]),
        _ => format!("{v:.1} {}", U[i]),
    }
}

pub fn tty() -> bool {
    console::user_attended()
}

pub fn width() -> usize {
    (console::Term::stdout().size().1 as usize).clamp(60, 110)
}

pub fn tilde(p: &Path) -> String {
    match dirs::home_dir() {
        Some(h) if p.starts_with(&h) => format!(
            "~{}",
            p.strip_prefix(&h)
                .map(|r| format!("/{}", r.display()))
                .unwrap_or_default()
                .trim_end_matches('/')
        ),
        _ => p.display().to_string(),
    }
}

pub fn ago(ts: i64) -> String {
    let s = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        - ts)
        .max(0);
    match s {
        0..=89 => "just now".into(),
        90..=5399 => format!("{} min ago", s / 60),
        5400..=129_599 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86_400),
    }
}

/// Doluluk oranına göre yeşil, sarı, kırmızı.
pub fn heat(frac: f64) -> Style {
    Style::new().fg(if frac < 0.70 {
        Color::Color256(78)
    } else if frac < 0.90 {
        Color::Color256(221)
    } else {
        Color::Color256(203)
    })
}

pub fn teal() -> Style {
    Style::new().fg(Color::Color256(80))
}

pub fn dim<D: std::fmt::Display>(d: D) -> StyledObject<D> {
    style(d).dim()
}

/// `frac` kadar dolu çubuk.
pub fn bar(frac: f64, width: usize, st: &Style) -> String {
    let f = frac.clamp(0.0, 1.0);
    let n = (f * width as f64 * 8.0).round() as usize; // 1/8 hücre çözünürlüğü
    let (full, part) = (n / 8, n % 8);
    const P: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
    let mut s = "█".repeat(full);
    if full < width {
        s.push_str(P[part]);
    }
    let used = s.chars().count();
    format!(
        "{}{}",
        st.apply_to(s),
        style("·".repeat(width.saturating_sub(used))).dim()
    )
}

/// Küçük dalga: marka şeridi.
pub fn wave(width: usize) -> String {
    const L: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    (0..width)
        .map(|i| {
            let x = i as f64 / 5.0;
            let v = ((x.sin() + (x * 0.55 + 1.3).sin() * 0.6) / 1.6 + 1.0) / 2.0;
            let c = 24 + (i * 26 / width.max(1)) as u8; // koyu maviden turkuaza
            style(L[((v * 7.0).round() as usize).min(7)])
                .color256(30 + (c % 12))
                .to_string()
        })
        .collect()
}

pub fn ok(msg: impl std::fmt::Display) {
    println!("  {} {msg}", style("✓").green().bold());
}
pub fn fail(msg: impl std::fmt::Display) {
    println!("  {} {msg}", style("✗").red().bold());
}
pub fn hint(msg: impl std::fmt::Display) {
    println!("  {} {msg}", style("→").cyan());
}
pub fn heading(title: &str) {
    println!("\n  {}", style(title).bold());
}
/// Komut önerisi: `ebb clean` gibi.
pub fn cmd(c: &str) -> StyledObject<String> {
    style(format!(" {c} ")).cyan().bold().on_color256(236)
}
