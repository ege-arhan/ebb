mod ui;

use clap::{
    CommandFactory, Parser, Subcommand,
    builder::styling::{AnsiColor, Styles},
};
use console::style;
use dialoguer::{Confirm, MultiSelect, theme::ColorfulTheme};
use ebb_core::{
    agents,
    catalog::{self, Finding, Safety},
    connect,
    journal::{Journal, Report},
    ops,
    plan::Plan,
    scan::Node,
};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use ui::{bar, dim, heading, hint, human, tilde};

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Cyan.on_default().bold())
    .usage(AnsiColor::Cyan.on_default().bold())
    .literal(AnsiColor::Green.on_default().bold())
    .placeholder(AnsiColor::Yellow.on_default());

const EXAMPLES: &str = "\x1b[1;36mExamples\x1b[0m
  ebb                        Dashboard: disk, what can ebb, AI storage
  ebb clean                  Pick what to clean (interactive), moves to the Trash
  ebb scan ~/Projects        Where did the space go?
  ebb connect                Connect Claude, Codex, Cursor, LM Studio … in one step
  ebb why ~                  What grew since last time
  ebb optimize               Win space back without deleting (clones duplicate copies)
  ebb system                 Health score, memory, heavy apps, startup items
  ebb undo <plan-id>         Restore a cleanup";

#[derive(Parser)]
#[command(name = "ebb", version, styles = STYLES, after_help = EXAMPLES,
    about = "Disk space, safe cleanups and AI storage. Nothing is deleted without your confirmation.")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Dashboard: disk usage, cleanable space, AI storage.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Scan a folder (default: home) and show where the space went.
    Scan {
        path: Option<PathBuf>,
        #[arg(long, default_value_t = 1)]
        depth: usize,
        #[arg(long)]
        json: bool,
    },
    /// Largest folders and files under a path.
    Top {
        path: Option<PathBuf>,
        #[arg(short, default_value_t = 15)]
        n: usize,
        #[arg(long)]
        json: bool,
    },
    /// What grew since the previous `why` of this path.
    Why {
        path: Option<PathBuf>,
        #[arg(long, default_value_t = 10)]
        top: usize,
        #[arg(long)]
        json: bool,
    },
    /// Cleanable caches on this machine, grouped by safety.
    Findings {
        #[arg(long)]
        json: bool,
    },
    /// Choose what to clean. Interactive in a terminal; moves items to the Trash after you confirm.
    Clean {
        /// Catalog ids to include (safe + review). Skips the picker.
        #[arg(long, value_delimiter = ',')]
        ids: Vec<String>,
        /// Explicit paths instead of the catalog.
        #[arg(long, num_args = 1..)]
        paths: Vec<PathBuf>,
        /// Apply without asking (for scripts).
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Apply a saved plan (moves items to the Trash).
    Apply {
        plan_id: String,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Restore an applied plan from the Trash.
    Undo {
        plan_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Recent plans and what they moved.
    Journal {
        #[arg(short, default_value_t = 10)]
        n: usize,
        #[arg(long)]
        json: bool,
    },
    /// Disk used by AI agents and local models.
    Agents {
        #[arg(long)]
        json: bool,
    },
    /// Connect AI assistants (Claude Code, Codex, Claude Desktop, LM Studio, Cursor, VS Code …) to Ebb.
    Connect {
        /// Client id to connect (see --list).
        id: Option<String>,
        /// Show detected clients and their status.
        #[arg(long)]
        list: bool,
        /// Connect every detected client without asking.
        #[arg(long)]
        all: bool,
        /// Remove the connection instead.
        #[arg(long)]
        remove: bool,
        #[arg(long)]
        json: bool,
    },
    /// Find duplicate files (identical content) under a path.
    Dupes {
        path: Option<PathBuf>,
        #[arg(long, default_value_t = 1)]
        min_mb: u64,
        #[arg(long)]
        json: bool,
    },
    /// Free space without deleting: turn duplicate copies into clones that share disk blocks (APFS, Btrfs, XFS).
    Optimize {
        path: Option<PathBuf>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Compress a folder in place (macOS APFS, Windows NTFS). Files open as before.
    Compress {
        path: PathBuf,
        /** Undo compression */
        #[arg(long)]
        undo: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Large files you have not opened in months, and old installers.
    Big {
        path: Option<PathBuf>,
        #[arg(long, default_value_t = 200)]
        min_mb: u64,
        #[arg(long, default_value_t = 180)]
        days: u64,
        #[arg(long)]
        json: bool,
    },
    /// System checkup: health score, memory, heavy apps, startup items.
    System {
        #[arg(long)]
        json: bool,
    },
    /// Heavy apps; quit one gracefully with --quit <name>.
    Apps {
        #[arg(long)]
        quit: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Items that start with your computer; toggle user items.
    Startup {
        #[arg(long)]
        disable: Option<String>,
        #[arg(long)]
        enable: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// What is in the Trash; restore Ebb's items or empty it.
    Trash {
        /// Restore these Trash ids (from `ebb trash --json`).
        #[arg(long, num_args = 1..)]
        restore: Vec<String>,
        /// Permanently delete these Trash ids.
        #[arg(long, num_args = 1..)]
        purge: Vec<String>,
        /// Empty the whole Trash (permanent).
        #[arg(long)]
        empty: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Explain a path: size, catalog match, protection.
    Explain {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// List catalog entries.
    Catalog {
        #[arg(long)]
        json: bool,
    },
    /// Print shell completions (bash, zsh, fish, powershell, elvish).
    Completions { shell: clap_complete::Shell },
    /// Start the MCP server on stdio (used by AI clients).
    Mcp,
}

fn tr() -> bool {
    std::env::var("LANG").is_ok_and(|l| l.starts_with("tr"))
}

fn emit<T: serde::Serialize>(json: bool, v: &T, text: impl FnOnce()) {
    if json {
        println!("{}", serde_json::to_string_pretty(v).unwrap());
    } else {
        text();
    }
}

fn home_or(p: Option<PathBuf>) -> PathBuf {
    p.or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn spinner(msg: &str) -> ProgressBar {
    if !ui::tty() {
        return ProgressBar::hidden();
    }
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&[
                "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█", "▇", "▆", "▅", "▄", "▃", "▂", "▁", " ",
            ]),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb.set_message(msg.to_string());
    pb
}

fn scan_with_spinner(path: &std::path::Path) -> ebb_core::Result<Node> {
    let pb = spinner(&format!("Scanning {}", tilde(path)));
    let started = std::time::Instant::now();
    let r = ops::scan_with(path, &AtomicBool::new(false), |p| {
        pb.set_message(format!(
            "Scanning {}  {} {} files {} {}",
            style(tilde(&p.current)).dim(),
            style("·").dim(),
            p.files,
            style("·").dim(),
            human(p.bytes)
        ));
    });
    pb.finish_and_clear();
    if let Ok(n) = &r
        && ui::tty()
    {
        println!(
            "  {} {} · {} files · {:.1}s",
            style("Scanned").dim(),
            style(human(n.size)).bold(),
            n.files,
            started.elapsed().as_secs_f32()
        );
    }
    r
}

fn print_tree(n: &Node, depth: usize, root_size: u64, prefix: &str, last: bool, top: bool) {
    let frac = if root_size == 0 {
        0.0
    } else {
        n.size as f64 / root_size as f64
    };
    let (branch, next_prefix) = if top {
        (String::new(), String::new())
    } else {
        (
            format!("{prefix}{}", if last { "└─ " } else { "├─ " }),
            format!("{prefix}{}", if last { "   " } else { "│  " }),
        )
    };
    let name = if n.is_dir {
        style(format!("{}/", n.name)).blue().bold().to_string()
    } else if n.name.starts_with('(') {
        style(&n.name).dim().italic().to_string()
    } else {
        n.name.clone()
    };
    println!(
        "  {:>9}  {}  {:>4.0}%  {}{}",
        style(human(n.size)).bold(),
        bar(frac, 12, &ui::heat(frac.sqrt() * 0.9)),
        frac * 100.0,
        style(&branch).dim(),
        name
    );
    if depth == 0 {
        return;
    }
    let shown: Vec<&Node> = n.children.iter().filter(|c| c.size > 0).take(12).collect();
    for (i, c) in shown.iter().enumerate() {
        let is_last = i + 1 == shown.len() && n.children.len() <= shown.len();
        print_tree(c, depth - 1, root_size, &next_prefix, is_last, false);
    }
    let rest = n
        .children
        .iter()
        .filter(|c| c.size > 0)
        .count()
        .saturating_sub(shown.len());
    if rest > 0 {
        let rest_size: u64 = n
            .children
            .iter()
            .filter(|c| c.size > 0)
            .skip(shown.len())
            .map(|c| c.size)
            .sum();
        println!(
            "  {:>9}  {}         {}{}",
            dim(human(rest_size)),
            " ".repeat(12),
            style(&next_prefix).dim(),
            dim(format!("… {rest} more"))
        );
    }
}

fn safety_label(s: Safety) -> console::StyledObject<&'static str> {
    match s {
        Safety::Safe => style("safe").green(),
        Safety::Review => style("review").yellow(),
        Safety::Never => style("protected").dim(),
    }
}

fn print_findings(f: &[Finding]) {
    for (s, title, sub) in [
        (
            Safety::Safe,
            "Rebuilds itself",
            "caches the app recreates on demand",
        ),
        (
            Safety::Review,
            "Look before you remove",
            "may cost you something, e.g. offline data or a long download",
        ),
        (
            Safety::Never,
            "Protected",
            "Ebb will never put these in a plan",
        ),
    ] {
        let rows: Vec<&Finding> = f.iter().filter(|x| x.safety == s).collect();
        if rows.is_empty() {
            continue;
        }
        let total: u64 = rows.iter().map(|x| x.size).sum();
        let max = rows.iter().map(|x| x.size).max().unwrap_or(1) as f64;
        let head = match s {
            Safety::Safe => style(title).green().bold(),
            Safety::Review => style(title).yellow().bold(),
            Safety::Never => style(title).dim().bold(),
        };
        println!(
            "\n  {}  {}  {}",
            head,
            style(human(total)).bold(),
            dim(format!("· {sub}"))
        );
        for x in rows {
            println!(
                "   {:>9}  {}  {:<26} {}",
                style(human(x.size)).bold(),
                bar(x.size as f64 / max, 10, &ui::teal()),
                x.label,
                dim(tilde(&x.path))
            );
            if let (Some(note), true) = (&x.note, s != Safety::Safe) {
                println!("   {:>9}  {:10}  {}", "", "", dim(format!("↳ {note}")));
            }
        }
    }
}

fn print_report(verb: &str, r: &Report) {
    println!();
    for i in &r.items {
        match &i.error {
            None => ui::ok(tilde(&i.path)),
            Some(e) => ui::fail(format!("{}  {}", tilde(&i.path), style(e).red())),
        }
    }
    let done = r.items.len() - r.failed;
    println!(
        "\n  {} {}  {}",
        style(verb).bold(),
        style(human(r.bytes)).green().bold(),
        dim(format!(
            "· {done} ok{}",
            if r.failed > 0 {
                format!(", {} failed", r.failed)
            } else {
                String::new()
            }
        ))
    );
}

fn dashboard(json: bool) -> ebb_core::Result<()> {
    let disk = ops::disk_usage();
    let findings = catalog::findings(&catalog::load(None)?, tr());
    let ai = agents::ai_storage();
    let safe: u64 = findings
        .iter()
        .filter(|f| f.safety == Safety::Safe)
        .map(|f| f.size)
        .sum();
    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"disk": disk, "reclaimable_safe": safe, "findings": findings, "ai": ai})).unwrap());
        return Ok(());
    }
    let w = ui::width().min(76);
    println!("\n  {}", ui::wave(w));
    println!(
        "  {}  {}",
        style("ebb").bold().cyan(),
        dim(format!(
            "v{} · disk space, safe cleanups, AI storage",
            env!("CARGO_PKG_VERSION")
        ))
    );
    if let Some(d) = &disk {
        let used = d.total - d.free;
        let frac = used as f64 / d.total as f64;
        println!(
            "\n  {}  {} free of {}",
            style(&d.mount).bold(),
            style(human(d.free)).bold().green(),
            human(d.total)
        );
        println!(
            "  {}  {}",
            bar(frac, w.min(60), &ui::heat(frac)),
            ui::heat(frac).apply_to(format!("{:.0}% used", frac * 100.0))
        );
    }
    println!();
    if safe > 0 {
        println!(
            "  {} {} can ebb safely  {}",
            style("↓").cyan().bold(),
            style(human(safe)).bold().green(),
            ui::cmd("ebb clean")
        );
    } else {
        println!("  {} Nothing safe to clean right now.", style("✓").green());
    }
    let top: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.safety == Safety::Safe)
        .take(5)
        .collect();
    if !top.is_empty() {
        let max = top[0].size as f64;
        heading("Biggest safe caches");
        for f in top {
            println!(
                "   {:>9}  {}  {:<26} {}",
                style(human(f.size)).bold(),
                bar(f.size as f64 / max, 10, &ui::teal()),
                f.label,
                dim(tilde(&f.path))
            );
        }
    }
    if !ai.is_empty() {
        let total: u64 = ai.iter().map(|a| a.size).sum();
        heading(&format!("AI tools on disk  {}", style(human(total)).bold()));
        let max = ai[0].size.max(1) as f64;
        for a in ai.iter().filter(|a| a.size > 1_000_000).take(5) {
            println!(
                "   {:>9}  {}  {:<26} {}",
                style(human(a.size)).bold(),
                bar(a.size as f64 / max, 10, &Style_for(a.kind)),
                a.label,
                dim(a.kind)
            );
        }
    }
    let clients = connect::clients();
    let todo = clients
        .iter()
        .filter(|c| c.installed && !c.connected)
        .count();
    println!();
    if todo > 0 {
        println!(
            "  {} {} AI assistant{} can see your disk  {}",
            style("★").yellow(),
            todo,
            if todo == 1 { "" } else { "s" },
            ui::cmd("ebb connect")
        );
    }
    hint(format!(
        "{}  where did the space go     {}  restore a cleanup",
        ui::cmd("ebb scan ~"),
        ui::cmd("ebb undo <id>")
    ));
    println!();
    Ok(())
}

#[allow(non_snake_case)]
fn Style_for(kind: &str) -> console::Style {
    if kind == "model" {
        console::Style::new().color256(214)
    } else {
        ui::teal()
    }
}

fn choose_and_clean(
    json: bool,
    yes: bool,
    ids: Vec<String>,
    paths: Vec<PathBuf>,
) -> ebb_core::Result<()> {
    let j = Journal::open_default()?;
    let plan: Plan = if !paths.is_empty() {
        Plan::from_paths(&paths)
    } else {
        let pb = spinner("Looking for caches");
        let all = catalog::findings(&catalog::load(None)?, tr());
        pb.finish_and_clear();
        let candidates: Vec<Finding> = all
            .into_iter()
            .filter(|f| f.safety != Safety::Never)
            .collect();
        let chosen: Vec<Finding> = if !ids.is_empty() {
            candidates
                .into_iter()
                .filter(|f| ids.contains(&f.entry_id))
                .collect()
        } else if ui::tty() && !yes && !json {
            if candidates.is_empty() {
                println!(
                    "\n  {} Nothing to clean. This machine is already light.\n",
                    style("✓").green()
                );
                return Ok(());
            }
            println!(
                "\n  {}  {}\n  {}\n",
                style("Choose what to clean").bold(),
                dim("space = toggle · enter = continue · esc = cancel"),
                dim("Safe items are pre-selected. “review” items are not.")
            );
            let items: Vec<String> = candidates
                .iter()
                .map(|f| {
                    format!(
                        "{:>9}  {} {:<9} {:<26} {}",
                        human(f.size),
                        "│",
                        match f.safety {
                            Safety::Safe => "safe",
                            _ => "review",
                        },
                        f.label,
                        dim(tilde(&f.path))
                    )
                })
                .collect();
            let defaults: Vec<bool> = candidates
                .iter()
                .map(|f| f.safety == Safety::Safe)
                .collect();
            let Some(picked) = MultiSelect::with_theme(&ColorfulTheme::default())
                .items(&items)
                .defaults(&defaults)
                .interact_opt()
                .map_err(|e| ebb_core::Error::Io(std::io::Error::other(e)))?
            else {
                println!("  Cancelled. Nothing changed.");
                return Ok(());
            };
            picked.into_iter().map(|i| candidates[i].clone()).collect()
        } else {
            candidates
                .into_iter()
                .filter(|f| f.safety == Safety::Safe)
                .collect()
        };
        let sel = ebb_core::plan::Select::Ids(chosen.iter().map(|f| f.entry_id.clone()).collect());
        Plan::from_findings(&chosen, &sel)
    };
    j.save_plan(&plan)?;

    let interactive = ui::tty() && !yes && !json;
    if interactive && !plan.items.is_empty() {
        heading("Plan");
        for i in &plan.items {
            println!(
                "   {:>9}  {}",
                style(human(i.size)).bold(),
                dim(tilde(&i.path))
            );
        }
        for (p, why) in &plan.skipped {
            println!(
                "   {} {}  {}",
                style("skipped").yellow(),
                tilde(p),
                dim(why)
            );
        }
        println!();
        if !Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Move {} item{} ({}) to the Trash?",
                plan.items.len(),
                if plan.items.len() == 1 { "" } else { "s" },
                human(plan.total)
            ))
            .default(true)
            .interact()
            .unwrap_or(false)
        {
            println!(
                "  Plan {} saved, nothing moved. Apply later: {}",
                plan.id,
                ui::cmd(&format!("ebb apply {} --yes", plan.id))
            );
            return Ok(());
        }
    }
    if yes || interactive {
        let r = j.apply(&plan.id)?;
        emit(json, &r, || {
            print_report("Moved to the Trash", &r);
            println!();
            hint(format!(
                "Changed your mind?  {}",
                ui::cmd(&format!("ebb undo {}", plan.id))
            ));
            hint(dim("Space comes back when you empty the Trash."));
            println!();
        });
        return Ok(());
    }
    emit(json, &plan, || {
        heading("Plan");
        plan.items.iter().for_each(|i| {
            println!(
                "   {:>9}  {}",
                style(human(i.size)).bold(),
                dim(tilde(&i.path))
            )
        });
        plan.skipped.iter().for_each(|(p, why)| {
            println!(
                "   {} {}  {}",
                style("skipped").yellow(),
                tilde(p),
                dim(why)
            )
        });
        println!(
            "\n  Plan {}  ·  {} items  ·  {}\n  Nothing changed. Apply with {}\n",
            style(&plan.id).bold(),
            plan.items.len(),
            style(human(plan.total)).bold(),
            ui::cmd(&format!("ebb apply {} --yes", plan.id))
        );
    });
    Ok(())
}

fn connect_cmd(
    id: Option<String>,
    list: bool,
    all: bool,
    remove: bool,
    json: bool,
) -> ebb_core::Result<()> {
    let exe = std::env::current_exe()?;
    let clients = connect::clients();
    if json || list {
        emit(json, &clients, || print_clients(&clients));
        return Ok(());
    }
    if remove {
        let id = id.ok_or_else(|| {
            ebb_core::Error::NotFound("which client? use `ebb connect --remove <id>`".into())
        })?;
        let o = connect::disconnect(&id)?;
        ui::ok(format!("{id}: {}", o.note));
        return Ok(());
    }
    let targets: Vec<String> = if let Some(id) = id {
        vec![id]
    } else if all || !ui::tty() {
        clients
            .iter()
            .filter(|c| c.installed && !c.connected)
            .map(|c| c.id.to_string())
            .collect()
    } else {
        print_clients(&clients);
        let pending: Vec<&connect::ClientInfo> = clients
            .iter()
            .filter(|c| c.installed && !c.connected)
            .collect();
        if pending.is_empty() {
            println!(
                "\n  {} Everything detected is already connected.\n",
                style("✓").green()
            );
            return Ok(());
        }
        println!(
            "\n  {}  {}\n",
            style("Connect which assistants?").bold(),
            dim("space = toggle · enter = connect")
        );
        let names: Vec<String> = pending
            .iter()
            .map(|c| format!("{:<28} {}", c.name, dim(c.note)))
            .collect();
        let Some(sel) = MultiSelect::with_theme(&ColorfulTheme::default())
            .items(&names)
            .defaults(&vec![true; names.len()])
            .interact_opt()
            .map_err(|e| ebb_core::Error::Io(std::io::Error::other(e)))?
        else {
            return Ok(());
        };
        sel.into_iter().map(|i| pending[i].id.to_string()).collect()
    };
    if targets.is_empty() {
        println!(
            "  Nothing to connect. Run {} to see what was detected.",
            ui::cmd("ebb connect --list")
        );
        return Ok(());
    }
    println!();
    let mut any = false;
    for t in &targets {
        match connect::connect(t, &exe) {
            Ok(o) => {
                any = true;
                ui::ok(format!("{:<18} {}", t, dim(o.note)));
                if let Some(b) = o.backup {
                    println!("      {}", dim(format!("backup: {}", tilde(&b))));
                }
            }
            Err(e) => ui::fail(format!("{:<18} {}", t, style(e).red())),
        }
    }
    if any {
        match connect::selftest(&exe) {
            Ok(n) => ui::ok(format!("Ebb MCP server answers with {n} tools")),
            Err(e) => ui::fail(format!("self-test failed: {e}")),
        }
        heading("Try asking your assistant");
        for q in [
            "What is using my disk space?",
            "Which AI tools take the most storage?",
            "Make a cleanup plan for safe caches (don't apply it).",
        ] {
            println!("   {} {}", style("›").cyan(), q);
        }
        println!(
            "\n  {}\n",
            dim("Assistants can scan, explain and plan. Only you can apply a plan (ebb apply).")
        );
    }
    Ok(())
}

fn print_clients(clients: &[connect::ClientInfo]) {
    heading("AI assistants");
    for c in clients {
        let state = if c.connected {
            style("● connected").green().to_string()
        } else if c.installed {
            style("○ ready").yellow().to_string()
        } else {
            style("· not found").dim().to_string()
        };
        println!(
            "   {:<28} {:<14} {}",
            if c.installed {
                style(c.name).bold().to_string()
            } else {
                dim(c.name).to_string()
            },
            state,
            dim(c.id)
        );
    }
}

fn run() -> ebb_core::Result<()> {
    let cli = Cli::parse();
    let Some(cmd) = cli.cmd else {
        return dashboard(false);
    };
    match cmd {
        Cmd::Status { json } => dashboard(json)?,
        Cmd::Scan { path, depth, json } => {
            let p = home_or(path);
            let mut node = scan_with_spinner(&p)?;
            if json {
                prune(&mut node, depth);
            }
            emit(json, &node, || {
                println!();
                print_tree(&node, depth, node.size, "", true, true);
                println!();
            });
        }
        Cmd::Top { path, n, json } => {
            let node = scan_with_spinner(&home_or(path))?;
            let (dirs, files) = ops::top(&node, n);
            emit(
                json,
                &serde_json::json!({"folders": dirs, "files": files}),
                || {
                    let max = dirs.first().map(|d| d.size).unwrap_or(1) as f64;
                    heading(&format!("Folders under {}", tilde(&node.path)));
                    dirs.iter().for_each(|d| {
                        println!(
                            "   {:>9}  {}  {}",
                            style(human(d.size)).bold(),
                            bar(d.size as f64 / max, 12, &ui::teal()),
                            tilde(&d.path)
                        )
                    });
                    let fmax = files.first().map(|d| d.size).unwrap_or(1) as f64;
                    heading("Largest files");
                    files.iter().for_each(|d| {
                        println!(
                            "   {:>9}  {}  {}",
                            style(human(d.size)).bold(),
                            bar(
                                d.size as f64 / fmax,
                                12,
                                &console::Style::new().color256(214)
                            ),
                            tilde(&d.path)
                        )
                    });
                    println!();
                },
            );
        }
        Cmd::Why { path, top, json } => {
            let pb = spinner("Comparing with the last scan");
            let r = ops::why(&Journal::open_default()?, &home_or(path), top)?;
            pb.finish_and_clear();
            emit(json, &r, || {
                println!("\n  {}  {}", style(tilde(&r.root)).bold(), human(r.total));
                match (r.since, r.total_before) {
                    (None, _) => {
                        hint("First run: baseline saved. Run again later to see what grew.")
                    }
                    (Some(ts), before) => {
                        println!(
                            "  {} {}",
                            dim(format!("since {}", ui::ago(ts))),
                            before
                                .map(|b| format!("({} → {})", human(b), human(r.total)))
                                .unwrap_or_default()
                        );
                        if r.growth.is_empty() {
                            ui::ok("Nothing grew.");
                        }
                        let max = r.growth.first().map(|g| g.delta as f64).unwrap_or(1.0);
                        for g in &r.growth {
                            println!(
                                "   {:>9}  {}  {}",
                                style(format!("+{}", human(g.delta as u64))).red().bold(),
                                bar(
                                    g.delta as f64 / max,
                                    10,
                                    &console::Style::new().color256(203)
                                ),
                                tilde(std::path::Path::new(&g.path))
                            );
                        }
                    }
                }
                println!();
            });
        }
        Cmd::Findings { json } => {
            let pb = spinner("Looking for caches");
            let f = catalog::findings(&catalog::load(None)?, tr());
            pb.finish_and_clear();
            emit(json, &f, || {
                print_findings(&f);
                let safe: u64 = f
                    .iter()
                    .filter(|x| x.safety == Safety::Safe)
                    .map(|x| x.size)
                    .sum();
                println!(
                    "\n  {} safe to clean  {}\n",
                    style(human(safe)).green().bold(),
                    ui::cmd("ebb clean")
                );
            });
        }
        Cmd::Clean {
            ids,
            paths,
            yes,
            json,
        } => choose_and_clean(json, yes, ids, paths)?,
        Cmd::Apply { plan_id, yes, json } => {
            let j = Journal::open_default()?;
            if !yes {
                let (plan, status) = j.plan(&plan_id)?;
                println!(
                    "\n  Plan {} ({}): {} items, {}.\n  Re-run with {} to move them to the Trash.\n",
                    style(&plan_id).bold(),
                    status,
                    plan.items.len(),
                    human(plan.total),
                    ui::cmd(&format!("ebb apply {plan_id} --yes"))
                );
                return Ok(());
            }
            let r = j.apply(&plan_id)?;
            emit(json, &r, || print_report("Moved to the Trash", &r));
        }
        Cmd::Undo { plan_id, json } => {
            let r = Journal::open_default()?.undo(&plan_id)?;
            emit(json, &r, || print_report("Restored", &r));
        }
        Cmd::Journal { n, json } => {
            let j = Journal::open_default()?;
            let l = j.list(n)?;
            emit(json, &l, || {
                if l.is_empty() {
                    println!("\n  Nothing applied yet. Plans you apply show up here.\n");
                    return;
                }
                heading("Journal");
                for p in &l {
                    let st = match p.status.as_str() {
                        "applied" => style("applied").green(),
                        "undone" => style("undone").dim(),
                        "failed" => style("failed").red(),
                        _ => style("planned").yellow(),
                    };
                    println!(
                        "   {}  {:<9} {:>9}  {}",
                        style(&p.id).bold(),
                        st,
                        human(p.total),
                        dim(ui::ago(p.created))
                    );
                    for e in j.items(&p.id).unwrap_or_default().iter().take(3) {
                        println!(
                            "   {}  {}",
                            " ".repeat(12 + 9 + 10),
                            dim(tilde(std::path::Path::new(&e.original)))
                        );
                    }
                }
                println!();
            });
        }
        Cmd::Agents { json } => {
            let a = agents::ai_storage();
            emit(json, &a, || {
                let total: u64 = a.iter().map(|s| s.size).sum();
                heading(&format!(
                    "AI tools and local models  {}",
                    style(human(total)).bold()
                ));
                let max = a.first().map(|s| s.size).unwrap_or(1).max(1) as f64;
                for s in &a {
                    println!(
                        "   {:>9}  {}  {:<24} {}",
                        style(human(s.size)).bold(),
                        bar(s.size as f64 / max, 12, &Style_for(s.kind)),
                        s.label,
                        dim(format!("{} · {}", s.kind, tilde(&s.path)))
                    );
                }
                println!();
                hint(format!(
                    "Let your assistant see this too  {}",
                    ui::cmd("ebb connect")
                ));
                println!();
            });
        }
        Cmd::Explain { path, json } => {
            let e = ops::explain(&path);
            emit(json, &e, || {
                println!("\n  {}", style(tilde(&e.path)).bold());
                println!("  {} · {} files", style(human(e.size)).bold(), e.files);
                if let Some(p) = &e.protected {
                    println!("  {} {}", style("protected").red().bold(), dim(p));
                }
                for (id, label, s) in &e.catalog {
                    println!("  catalog: {} ({}) {}", id, label, safety_label(*s));
                }
                println!();
            });
        }
        Cmd::Catalog { json } => {
            let c = catalog::load(None)?;
            emit(json, &c, || {
                println!();
                for e in &c {
                    println!(
                        "   {:<28} {:<9} {}",
                        e.id,
                        dim(&e.category),
                        match e.safety {
                            Safety::Safe => style("safe").green(),
                            Safety::Review => style("review").yellow(),
                            Safety::Never => style("protected").dim(),
                        }
                    );
                }
                println!("\n  {}\n", dim(format!("{} recipes", c.len())));
            });
        }
        Cmd::Connect {
            id,
            list,
            all,
            remove,
            json,
        } => connect_cmd(id, list, all, remove, json)?,
        Cmd::Dupes { path, min_mb, json } => {
            let root = home_or(path);
            let mut groups = find_dupes(&root, min_mb)?;
            ebb_core::optimize::mark_optimized(&Journal::open_default()?, &mut groups);
            emit(json, &groups, || print_dupes(&groups));
        }
        Cmd::Optimize { path, yes, json } => {
            let root = home_or(path);
            if !ebb_core::optimize::clone_supported(&root) {
                println!(
                    "\n  This disk can't share blocks between files (needs APFS, Btrfs or XFS).\n  Use {} to see the copies and remove extra ones.\n",
                    ui::cmd("ebb dupes")
                );
                return Ok(());
            }
            let j = Journal::open_default()?;
            let mut groups = find_dupes(&root, 1)?;
            ebb_core::optimize::mark_optimized(&j, &mut groups);
            let todo: Vec<_> = groups
                .into_iter()
                .filter(|g| !g.optimized && g.reclaimable > 0)
                .collect();
            let total: u64 = todo.iter().map(|g| g.reclaimable).sum();
            if todo.is_empty() {
                println!(
                    "\n  {} Nothing to optimize. Copies already share space.\n",
                    style("✓").green()
                );
                return Ok(());
            }
            if !json {
                print_dupes(&todo);
            }
            let go = yes
                || (ui::tty()
                    && !json
                    && Confirm::with_theme(&ColorfulTheme::default())
                        .with_prompt(format!(
                            "Turn {} copies into clones and win back {}? Nothing is deleted.",
                            todo.iter().map(|g| g.files.len() - 1).sum::<usize>(),
                            human(total)
                        ))
                        .default(true)
                        .interact()
                        .unwrap_or(false));
            if !go {
                println!("  Nothing changed. Run with --yes to optimize.");
                return Ok(());
            }
            let pb = spinner("Cloning copies");
            let r = ebb_core::optimize::dedupe_with_clones(&j, &todo)?;
            pb.finish_and_clear();
            emit(json, &r, || {
                println!(
                    "\n  {} {} won back · {} files now share space · nothing deleted",
                    style("✓").green().bold(),
                    style(human(r.saved)).green().bold(),
                    r.files
                );
                for (p, why) in r.skipped.iter().take(5) {
                    println!(
                        "   {} {}  {}",
                        style("skipped").yellow(),
                        tilde(p),
                        dim(why)
                    );
                }
                println!();
            });
        }
        Cmd::Compress {
            path,
            undo,
            yes,
            json,
        } => {
            let go = yes
                || json
                || !ui::tty()
                || Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(format!(
                        "{} {} in place?",
                        if undo { "Decompress" } else { "Compress" },
                        tilde(&path)
                    ))
                    .default(true)
                    .interact()
                    .unwrap_or(false);
            if !go {
                return Ok(());
            }
            let pb = spinner(if undo { "Decompressing" } else { "Compressing" });
            let r = if undo {
                ebb_core::optimize::decompress(&path)?
            } else {
                ebb_core::optimize::compress(&path)?
            };
            pb.finish_and_clear();
            emit(json, &r, || {
                let saved = r.before.saturating_sub(r.after);
                println!(
                    "\n  {} {}  {} → {}{}\n",
                    style("✓").green().bold(),
                    tilde(&r.path),
                    human(r.before),
                    human(r.after),
                    if saved > 0 {
                        format!("  ·  {} saved", style(human(saved)).green().bold())
                    } else {
                        String::new()
                    }
                );
            });
        }
        Cmd::Big {
            path,
            min_mb,
            days,
            json,
        } => {
            let pb = spinner("Looking for big, forgotten files");
            let large = ebb_core::finders::large_old_files(
                &home_or(path),
                min_mb * 1_000_000,
                days,
                40,
                &AtomicBool::new(false),
            )?;
            let inst = ebb_core::finders::old_installers(14);
            pb.finish_and_clear();
            emit(
                json,
                &serde_json::json!({"large": large, "installers": inst}),
                || {
                    for (title, rows) in [
                        ("Old installers", &inst),
                        ("Large files not opened in a while", &large),
                    ] {
                        heading(title);
                        if rows.is_empty() {
                            println!("   {}", dim("nothing here"));
                        }
                        for f in rows.iter().take(20) {
                            println!(
                                "   {:>9}  {:>5}d  {}",
                                style(human(f.size)).bold(),
                                f.age_days,
                                dim(tilde(&f.path))
                            );
                        }
                    }
                    println!(
                        "\n  Move some to the Trash with {}\n",
                        ui::cmd("ebb clean --paths <file> …")
                    );
                },
            );
        }
        Cmd::System { json } => {
            let pb = spinner("Checking your system");
            let h = ebb_core::system::health();
            let apps = ebb_core::system::top_apps(8);
            pb.finish_and_clear();
            emit(
                json,
                &serde_json::json!({"health": h, "apps": apps}),
                || {
                    let tone = if h.score >= 80 {
                        console::Style::new().green()
                    } else if h.score >= 55 {
                        console::Style::new().yellow()
                    } else {
                        console::Style::new().red()
                    };
                    println!(
                        "\n  {}  {}  {}",
                        style("Health").bold(),
                        tone.apply_to(format!("{}/100", h.score)).bold(),
                        bar(h.score as f64 / 100.0, 30, &tone)
                    );
                    let m = &h.memory;
                    println!(
                        "  Memory  {} of {} in use · pressure {:.0}% · swap {}",
                        style(human(m.used)).bold(),
                        human(m.total),
                        m.pressure * 100.0,
                        human(m.swap_used)
                    );
                    heading("Tips");
                    for tip in &h.tips {
                        let mark = match tip.severity {
                            "good" => style("✓").green(),
                            "warn" => style("!").yellow(),
                            "bad" => style("✗").red(),
                            _ => style("•").cyan(),
                        };
                        println!(
                            "   {} {}\n     {}",
                            mark,
                            style(&tip.title).bold(),
                            dim(&tip.body)
                        );
                    }
                    heading("Heavy apps");
                    let max = apps.first().map(|a| a.memory).unwrap_or(1) as f64;
                    for a in &apps {
                        println!(
                            "   {:>9}  {}  {:<26} {}",
                            style(human(a.memory)).bold(),
                            bar(a.memory as f64 / max, 10, &ui::teal()),
                            a.name,
                            dim(format!("cpu {:.1}%", a.cpu))
                        );
                    }
                    println!(
                        "\n  {}  quit an app     {}  startup items\n",
                        ui::cmd("ebb apps --quit <name>"),
                        ui::cmd("ebb startup")
                    );
                },
            );
        }
        Cmd::Apps { quit, yes, json } => {
            let apps = ebb_core::system::top_apps(20);
            match quit {
                None => emit(json, &apps, || {
                    heading("Apps by memory");
                    for a in &apps {
                        println!(
                            "   {:>9}  {:<28} {}{}",
                            style(human(a.memory)).bold(),
                            a.name,
                            dim(format!("cpu {:.1}%", a.cpu)),
                            if a.quittable { "" } else { "  (system)" }
                        );
                    }
                    println!();
                }),
                Some(name) => {
                    let a = apps
                        .iter()
                        .find(|a| a.name.eq_ignore_ascii_case(&name))
                        .ok_or_else(|| ebb_core::Error::NotFound(format!("app {name}")))?;
                    let go = yes
                        || (ui::tty()
                            && Confirm::with_theme(&ColorfulTheme::default())
                                .with_prompt(format!(
                                    "Ask {} to quit? Save your work first.",
                                    a.name
                                ))
                                .default(false)
                                .interact()
                                .unwrap_or(false));
                    if go {
                        let n = ebb_core::system::quit_app(&a.pids)?;
                        ui::ok(format!("Asked {} to quit ({n} processes)", a.name));
                    }
                }
            }
        }
        Cmd::Startup {
            disable,
            enable,
            json,
        } => {
            if let Some(id) = disable.as_deref() {
                ebb_core::system::set_startup_enabled(id, false)?;
                ui::ok(format!("Disabled {id}"));
            } else if let Some(id) = enable.as_deref() {
                ebb_core::system::set_startup_enabled(id, true)?;
                ui::ok(format!("Enabled {id}"));
            } else {
                let items = ebb_core::system::startup_items();
                emit(json, &items, || {
                    heading("Starts with your computer");
                    for s in &items {
                        let st = if s.enabled {
                            style("on ").green()
                        } else {
                            style("off").dim()
                        };
                        println!(
                            "   {}  {:<40} {}",
                            st,
                            s.name,
                            dim(if s.can_toggle {
                                s.id.clone()
                            } else {
                                format!("{} (read only)", s.scope)
                            })
                        );
                    }
                    println!(
                        "\n  {}\n",
                        dim("Turn one off with: ebb startup --disable <id>")
                    );
                });
            }
        }
        Cmd::Trash {
            restore,
            purge,
            empty,
            yes,
            json,
        } => {
            let j = Journal::open_default()?;
            if !restore.is_empty() {
                let n = ebb_core::trashbin::restore(&j, &restore)?;
                ui::ok(format!(
                    "Restored {n} item{}",
                    if n == 1 { "" } else { "s" }
                ));
                return Ok(());
            }
            if empty || !purge.is_empty() {
                let what = if empty {
                    "Empty the Trash".to_string()
                } else {
                    format!("Permanently delete {} item(s)", purge.len())
                };
                let go = yes
                    || (ui::tty()
                        && Confirm::with_theme(&ColorfulTheme::default())
                            .with_prompt(format!("{what}? This cannot be undone."))
                            .default(false)
                            .interact()
                            .unwrap_or(false));
                if !go {
                    println!("  Nothing changed. Add --yes to confirm.");
                    return Ok(());
                }
                let bytes = if empty {
                    ebb_core::trashbin::empty(&j)?
                } else {
                    ebb_core::trashbin::purge(&j, &purge)?
                };
                ui::ok(format!("{} freed", style(human(bytes)).green().bold()));
                return Ok(());
            }
            let pb = spinner("Reading the Trash");
            let items = ebb_core::trashbin::list(&j)?;
            pb.finish_and_clear();
            emit(json, &items, || {
                let total: u64 = items.iter().map(|e| e.size).sum();
                heading(&format!(
                    "Trash  {} · {} items",
                    style(human(total)).bold(),
                    items.len()
                ));
                for e in items.iter().take(30) {
                    let tag = if e.by_ebb {
                        style("ebb ").cyan().to_string()
                    } else {
                        "    ".into()
                    };
                    println!(
                        "   {:>9}  {}{:<34} {}",
                        style(human(e.size)).bold(),
                        tag,
                        e.name,
                        dim(e.original.as_ref().map(|o| tilde(o)).unwrap_or_default())
                    );
                }
                println!(
                    "\n  {}  restore (Ebb items)     {}  free the space\n",
                    ui::cmd("ebb trash --restore <id>"),
                    ui::cmd("ebb trash --empty")
                );
            });
        }
        Cmd::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "ebb", &mut std::io::stdout())
        }
        Cmd::Mcp => ebb_mcp::serve_stdio()?,
    }
    Ok(())
}

fn find_dupes(
    root: &std::path::Path,
    min_mb: u64,
) -> ebb_core::Result<Vec<ebb_core::dupes::DupGroup>> {
    let pb = spinner(&format!("Comparing files under {}", tilde(root)));
    let r =
        ebb_core::dupes::find_duplicates(root, min_mb * 1_000_000, &AtomicBool::new(false), |p| {
            pb.set_message(format!(
                "{} {} files · {}",
                if p.stage == "hashing" {
                    "Comparing"
                } else {
                    "Listing"
                },
                p.files,
                human(p.bytes)
            ));
        });
    pb.finish_and_clear();
    r
}

fn print_dupes(groups: &[ebb_core::dupes::DupGroup]) {
    let total: u64 = groups
        .iter()
        .filter(|g| !g.optimized)
        .map(|g| g.reclaimable)
        .sum();
    heading(&format!(
        "Duplicate files  {} can be won back",
        style(human(total)).green().bold()
    ));
    for g in groups.iter().take(20) {
        let name = g.files[0]
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mark = if g.optimized {
            style("✓ shared".to_string()).dim()
        } else {
            style(format!("+{}", human(g.reclaimable))).green()
        };
        println!(
            "   {:>10}  {} × {:<32} {}",
            mark,
            g.files.len(),
            name,
            dim(tilde(&g.files[1]))
        );
    }
    if groups.len() > 20 {
        println!("   {}", dim(format!("… {} more groups", groups.len() - 20)));
    }
    println!();
}

fn prune(n: &mut Node, depth: usize) {
    if depth == 0 {
        n.children.clear();
    } else {
        n.children.iter_mut().for_each(|c| prune(c, depth - 1));
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("\n  {} {e}\n", style("error").red().bold());
        std::process::exit(1);
    }
}
