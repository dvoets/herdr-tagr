//! herdr-tagr - concise, icon-first tab titles for herdr.
//!
//! `daemon` is what the plugin's startup hook runs. The other subcommands are
//! the plugin actions and a couple of debugging aids.

mod config;
mod detect;
mod engine;
mod git;
mod icons;
mod label;
mod socket;
mod state;
mod transport;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use engine::Engine;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Events worth recomputing on. `pane.updated` is by far the noisiest (it fires
/// as panes produce output), which is exactly why the daemon debounces.
const EVENTS: &[&str] = &[
    "pane.created",
    "pane.updated",
    "pane.closed",
    "pane.exited",
    "pane.focused",
    "pane.agent_detected",
    "tab.created",
    "tab.closed",
    "tab.focused",
    "tab.renamed",
    "workspace.focused",
    "layout.updated",
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("daemon");

    let result = match cmd {
        "daemon" => daemon(),
        "refresh" => refresh(),
        "adopt" => set_adoption(true),
        "release" => set_adoption(false),
        "print" => print_labels(),
        "doctor" => doctor(),
        "--version" | "-V" | "version" => {
            println!("herdr-tagr {VERSION}");
            Ok(())
        }
        "--help" | "-h" | "help" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unknown command {other:?}; try --help")),
    };

    if let Err(e) = result {
        eprintln!("herdr-tagr: {e}");
        std::process::exit(1);
    }
}

fn print_help() {
    println!(
        "herdr-tagr {VERSION} - concise, icon-first tab titles for herdr

Usage: herdr-tagr <command>

Commands:
  daemon    Subscribe to herdr events and keep tab titles up to date (default)
  refresh   Recompute every tab title once and exit
  adopt     Start auto-titling the current tab
  release   Stop auto-titling the current tab
  print     Show what each tab would be titled, without changing anything
  doctor    Explain how the current pane's title is derived"
    );
}

/// Long-lived loop: an event reader thread raises a flag, the main thread
/// coalesces the burst and runs one pass.
fn daemon() -> Result<(), String> {
    let mut engine = Engine::new()?;
    let debug = engine.cfg.general.debug;

    // Title everything once at startup, before any event arrives.
    match engine.pass() {
        Ok(r) if debug => eprintln!("herdr-tagr: startup pass renamed {}", r.renamed.len()),
        Ok(_) => {}
        Err(e) => eprintln!("herdr-tagr: startup pass failed: {e}"),
    }

    let dirty = Arc::new(AtomicBool::new(false));
    spawn_event_reader(
        Arc::clone(&dirty),
        debug,
        engine.cfg.general.socket_path.clone(),
    );

    let debounce = Duration::from_millis(engine.cfg.general.debounce_ms);
    let poll = engine.cfg.general.poll_ms;
    let tick = Duration::from_millis(25);

    loop {
        // Wait for the event thread, or for the fallback poll to come due.
        let mut waited = 0u64;
        while !dirty.load(Ordering::Relaxed) {
            thread::sleep(tick);
            waited += tick.as_millis() as u64;
            if poll > 0 && waited >= poll {
                break;
            }
        }
        // Let the rest of the burst land before doing any work.
        thread::sleep(debounce);
        dirty.store(false, Ordering::Relaxed);

        if engine.reload_config_if_changed() && debug {
            eprintln!("herdr-tagr: config reloaded");
        }
        if !engine.ready() {
            continue;
        }
        match engine.pass() {
            Ok(report) => {
                if debug {
                    for (tab, label) in &report.renamed {
                        eprintln!("herdr-tagr: {tab} -> {label}");
                    }
                    for e in &report.errors {
                        eprintln!("herdr-tagr: {e}");
                    }
                }
            }
            Err(e) => {
                eprintln!("herdr-tagr: pass failed: {e}");
                thread::sleep(Duration::from_secs(1));
            }
        }
    }
}

/// Reads the event stream forever, reconnecting with backoff if herdr restarts.
fn spawn_event_reader(dirty: Arc<AtomicBool>, debug: bool, socket_path: String) {
    thread::spawn(move || {
        let mut backoff = Duration::from_millis(250);
        loop {
            let client = match crate::socket::Client::from_env(Some(&socket_path)) {
                Ok(c) => c,
                Err(e) => {
                    if debug {
                        eprintln!("herdr-tagr: {e}");
                    }
                    thread::sleep(backoff);
                    backoff = (backoff * 2).min(Duration::from_secs(10));
                    continue;
                }
            };
            match client.subscribe(EVENTS) {
                Ok(events) => {
                    backoff = Duration::from_millis(250);
                    for _ in events {
                        dirty.store(true, Ordering::Relaxed);
                    }
                    if debug {
                        eprintln!("herdr-tagr: event stream closed, reconnecting");
                    }
                }
                Err(e) => {
                    if debug {
                        eprintln!("herdr-tagr: subscribe failed: {e}");
                    }
                }
            }
            // The stream ended: herdr restarted, or the socket went away.
            thread::sleep(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(10));
            dirty.store(true, Ordering::Relaxed);
        }
    });
}

fn refresh() -> Result<(), String> {
    let mut engine = Engine::new()?;
    let report = engine.pass()?;
    println!(
        "herdr-tagr: retitled {} tab(s), skipped {}",
        report.renamed.len(),
        report.skipped
    );
    for e in &report.errors {
        eprintln!("herdr-tagr: {e}");
    }
    Ok(())
}

/// `adopt` / `release` act on the tab the action was invoked from.
fn set_adoption(adopt: bool) -> Result<(), String> {
    let tab = std::env::var("HERDR_TAB_ID")
        .map_err(|_| "no HERDR_TAB_ID in the environment; run this from a tab".to_string())?;
    let mut engine = Engine::new()?;
    if adopt {
        engine.state.adopt(&tab);
    } else {
        engine.state.exclude(&tab);
    }
    engine.state.save();
    let report = engine.pass()?;
    println!(
        "herdr-tagr: {} {tab}{}",
        if adopt { "auto-titling" } else { "released" },
        report
            .renamed
            .iter()
            .find(|(t, _)| *t == tab)
            .map(|(_, l)| format!(" -> {l}"))
            .unwrap_or_default()
    );
    Ok(())
}

fn print_labels() -> Result<(), String> {
    let mut engine = Engine::new()?;
    for (tab, pane, label, managed) in engine.preview()? {
        println!(
            "{:<8} {:<8} {:<3} {:<28} (now: {})",
            tab.tab_id,
            pane.pane_id,
            if managed { "yes" } else { "no" },
            label,
            tab.label.unwrap_or_default()
        );
    }
    Ok(())
}

/// Explains one pane end to end: processes seen, app chosen, label produced.
fn doctor() -> Result<(), String> {
    let pane_id = std::env::var("HERDR_PANE_ID")
        .map_err(|_| "no HERDR_PANE_ID in the environment; run this inside a pane".to_string())?;
    let mut engine = Engine::new()?;

    println!("pane:    {pane_id}");
    println!(
        "config:  {}",
        config::Config::path()
            .map(|p| p.display().to_string())
            .unwrap_or("(defaults)".into())
    );

    let raw = engine.process_info_raw(&pane_id)?;
    let info: detect::ProcessInfo = raw
        .get("process_info")
        .cloned()
        .and_then(|p| serde_json::from_value(p).ok())
        .unwrap_or_default();

    println!("\nforeground processes:");
    for p in &info.foreground_processes {
        let rank = engine
            .apps()
            .iter()
            .filter(|a| {
                a.matches.iter().any(|m| {
                    m.eq_ignore_ascii_case(&p.name)
                        || p.args()
                            .first()
                            .map(|a0| a0.rsplit('/').next().unwrap_or(a0).eq_ignore_ascii_case(m))
                            .unwrap_or(false)
                })
            })
            .map(|a| format!("{} rank {}", a.id, a.rank))
            .next()
            .unwrap_or_else(|| "unranked".to_string());
        let fg = if Some(p.pid) == info.foreground_process_group_id {
            " <- foreground group"
        } else {
            ""
        };
        let cwd = p.cwd.as_deref().unwrap_or("-");
        println!("  {:<6} {:<18} {rank}{fg}\n         {cwd}", p.pid, p.name);
    }

    if let Some(pid) = info.shell_pid {
        println!("  shell pid {pid}");
    }

    let detected = detect::detect(&info, engine.apps());
    println!(
        "\nwinner:  {} (rank {})",
        detected.app.id, detected.app.rank
    );
    if let Some(host) = &detected.ssh_host {
        println!("ssh host: {host}");
    }

    for (tab, pane, lbl, managed) in engine.preview()? {
        if pane.pane_id == pane_id {
            if let Some(dir) = pane.foreground_cwd.as_deref().or(pane.cwd.as_deref()) {
                println!("cwd:     {dir}");
                let gcfg = engine.cfg.clone();
                if let Some(repo) = engine.git_cache().repo(std::path::Path::new(dir), &gcfg) {
                    println!("repo:    {} ({:?})", repo.root.display(), repo.head);
                }
            }
            println!("label:   {lbl}");
            println!(
                "tab:     {} (currently {:?}, managed: {managed})",
                tab.tab_id,
                tab.label.unwrap_or_default()
            );
        }
    }
    Ok(())
}
