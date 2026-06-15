mod app;
mod config;
mod keychain;
mod ui;

use std::io;
use std::sync::Arc;

use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;
use crate::config::Config;
use crate::keychain::RealKeychain;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str());

    match cmd {
        None | Some("tui") | Some("interactive") => run_tui(),
        Some("help") | Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some("install") => {
            print_install();
            Ok(())
        }
        Some("status") | Some("st") => {
            cli_status();
            Ok(())
        }
        Some("init") => {
            cli_init(&args[2..]);
            Ok(())
        }

        // CLI commands (delegate to the same backend)
        Some("set") | Some("s") => {
            cli_set(&args[2..]);
            Ok(())
        }
        Some("get") | Some("g") => {
            cli_get(&args[2..]);
            Ok(())
        }
        Some("list") | Some("ls") | Some("l") => {
            cli_list(&args[2..]);
            Ok(())
        }
        Some("load") => {
            cli_load(&args[2..]);
            Ok(())
        }
        Some("cat") | Some("c") => {
            cli_cat(&args[2..]);
            Ok(())
        }
        Some("cp") => {
            cli_cp(&args[2..]);
            Ok(())
        }
        Some("delete") | Some("del") | Some("d") => {
            cli_delete(&args[2..]);
            Ok(())
        }
        Some("pull") | Some("pl") => {
            cli_pull(&args[2..]);
            Ok(())
        }
        Some("push") | Some("ph") => {
            cli_push(&args[2..]);
            Ok(())
        }
        Some("run") | Some("r") => {
            cli_run(&args[2..]);
            Ok(())
        }
        Some("rm") => {
            cli_rm_project(&args[2..]);
            Ok(())
        }

        Some(other) => {
            eprintln!("ke: unknown command '{other}'");
            eprintln!("Try 'ke help'");
            std::process::exit(1);
        }
    }
}

/// Validate a project name: alphanumeric, hyphens, underscores, no path separators.
pub fn validate_project_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("project name cannot be empty".into());
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("project name contains invalid characters".into());
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err("project name must be alphanumeric (hyphens and underscores allowed)".into());
    }
    Ok(())
}

/// Validate an env-var key name: uppercase, alphanumeric, underscores.
pub fn validate_key_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("key name cannot be empty".into());
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("key name must be ASCII alphanumeric or underscore".into());
    }
    Ok(())
}

fn print_help() {
    println!(
        r#"ke — macOS Keychain secrets manager

Usage:
  ke               Interactive TUI (default)
  ke help          Show this message

CLI commands:
  ke set <project> <key> [val]    Store a secret
  ke get <project> <key>          Print a secret
  ke ls [project]                 List projects or keys
  ke load <project>               Export KEY=val lines
  ke cat <project>                Print .env format
  ke cp <project> <key>           Copy to clipboard
  ke delete <project> <key>       Remove a secret
  ke pull <project>               Write .env.local
  ke push <project> [file]        Import .env file
  ke rm <project>                 Remove project
  ke run <project> -- <cmd>       Run with secrets
  ke status                       Show sync status and missing values
  ke init --icloud                Enable iCloud Drive sync for key names"#
    );
}

fn print_install() {
    println!(
        "ke is already installed at: {}",
        std::env::current_exe().unwrap_or_default().display()
    );
    println!();
    println!("For the bash version (with fzf TUI), run:");
    println!("  curl -fsSL https://raw.githubusercontent.com/themuuln/ke/main/ke \\");
    println!("    | sudo tee /usr/local/bin/ke >/dev/null && sudo chmod +x /usr/local/bin/ke");
}

// ═══════════════════════════════════════════════════════════════════════
// TUI Mode
// ═══════════════════════════════════════════════════════════════════════

fn run_tui() -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let config = Config::load()?;
    let mut app = App::new(config, Arc::new(RealKeychain::new()))?;
    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("Error: {:#}", e);
    }
    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    terminal.draw(|frame| ui::draw(frame, app))?;
    while app.handle_event()? {
        terminal.draw(|frame| ui::draw(frame, app))?;
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// CLI Commands
// ═══════════════════════════════════════════════════════════════════════

fn cli_set(args: &[String]) {
    if args.len() < 2 {
        eprintln!("Usage: ke set <project> <key> [value]");
        std::process::exit(1);
    }
    let project = &args[0];
    let key = &args[1];
    if let Err(msg) = validate_project_name(project) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }
    if let Err(msg) = validate_key_name(key) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }

    let val = if args.len() > 2 {
        args[2].clone()
    } else {
        use std::io::Write;
        print!("Value (hidden): ");
        std::io::stdout().flush().expect("failed to flush stdout");
        rpassword::read_password().unwrap_or_default()
    };

    if val.is_empty() {
        eprintln!("Error: value cannot be empty");
        std::process::exit(1);
    }

    match keychain::Keychain::set(project, key, &val) {
        Ok(()) => {
            let config = Config::load().expect("failed to load config");
            let _ = config.add_key(project, key);
            eprintln!("Saved {}:{}", project, key);
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

fn cli_get(args: &[String]) {
    if args.len() < 2 {
        eprintln!("Usage: ke get <project> <key>");
        std::process::exit(1);
    }
    let project = &args[0];
    let key = &args[1];
    if let Err(msg) = validate_project_name(project) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }
    if let Err(msg) = validate_key_name(key) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }
    let val = keychain::Keychain::get(project, key).unwrap_or_else(|| {
        eprintln!("Error: {project}:{key} not found");
        std::process::exit(1);
    });
    println!("{val}");
}

fn cli_list(args: &[String]) {
    let config = Config::load().expect("failed to load config");
    if args.is_empty() {
        let projects = config.list_projects().unwrap_or_default();
        if projects.is_empty() {
            eprintln!("No projects in Keychain.");
        } else {
            println!("Projects in Keychain:");
            for p in &projects {
                println!("  {p}");
            }
        }
    } else {
        let project = &args[0];
        if let Err(msg) = validate_project_name(project) {
            eprintln!("Error: {msg}");
            std::process::exit(1);
        }
        let keys = config.list_keys(project).unwrap_or_default();
        if keys.is_empty() {
            eprintln!("No secrets for '{project}'.");
        } else {
            println!("Secrets for '{project}':");
            for k in &keys {
                println!("  {k}");
            }
        }
    }
}

fn cli_load(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: ke load <project>");
        std::process::exit(1);
    }
    let project = &args[0];
    if let Err(msg) = validate_project_name(project) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }
    let config = Config::load().expect("failed to load config");
    let keys = config.list_keys(project).unwrap_or_default();
    println!("# {project} — from macOS Keychain");
    for k in &keys {
        if let Some(v) = keychain::Keychain::get(project, k) {
            println!("export {k}={v}");
        }
    }
}

fn cli_cat(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: ke cat <project>");
        std::process::exit(1);
    }
    let project = &args[0];
    if let Err(msg) = validate_project_name(project) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }
    let config = Config::load().expect("failed to load config");
    let keys = config.list_keys(project).unwrap_or_default();
    println!("# {project} — from macOS Keychain");
    for k in &keys {
        if let Some(v) = keychain::Keychain::get(project, k) {
            println!("{k}={v}");
        }
    }
}

fn cli_cp(args: &[String]) {
    if args.len() < 2 {
        eprintln!("Usage: ke cp <project> <key>");
        std::process::exit(1);
    }
    let project = &args[0];
    let key = &args[1];
    if let Err(msg) = validate_project_name(project) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }
    if let Err(msg) = validate_key_name(key) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }
    let val = keychain::Keychain::get(project, key).unwrap_or_else(|| {
        eprintln!("Error: {project}:{key} not found");
        std::process::exit(1);
    });

    // Use pbcopy
    let mut cmd = std::process::Command::new("pbcopy");
    cmd.stdin(std::process::Stdio::piped());
    if let Ok(mut child) = cmd.spawn() {
        use std::io::Write;
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(val.as_bytes());
        }
        let _ = child.wait();
    }
    eprintln!("Copied {project}:{key} to clipboard!");
}

fn cli_delete(args: &[String]) {
    if args.len() < 2 {
        eprintln!("Usage: ke delete <project> <key>");
        std::process::exit(1);
    }
    let project = &args[0];
    let key = &args[1];
    if let Err(msg) = validate_project_name(project) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }
    if let Err(msg) = validate_key_name(key) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }
    match keychain::Keychain::delete(project, key) {
        Ok(()) => {
            let config = Config::load().expect("failed to load config");
            let _ = config.remove_key(project, key);
            eprintln!("Deleted {project}:{key}");
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

fn cli_pull(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: ke pull <project>");
        std::process::exit(1);
    }
    let project = &args[0];
    if let Err(msg) = validate_project_name(project) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }
    let config = Config::load().expect("failed to load config");
    let keys = config.list_keys(project).unwrap_or_default();
    let out = std::env::current_dir()
        .expect("failed to get current dir")
        .join(".env.local");
    let mut content = format!("# {project} — pulled from Keychain\n");
    content.push_str("# WARNING: contains secrets — never commit!\n\n");
    let mut count = 0;
    for k in &keys {
        if let Some(v) = keychain::Keychain::get(project, k) {
            content.push_str(&format!("{k}={v}\n"));
            count += 1;
        }
    }
    std::fs::write(&out, content).expect("failed to write .env.local");
    eprintln!("Wrote {} ({count} secrets)", out.display());
}

fn cli_push(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: ke push <project> [file]");
        std::process::exit(1);
    }
    let project = &args[0];
    if let Err(msg) = validate_project_name(project) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }
    let file = args.get(1).map(|s| s.as_str()).unwrap_or(".env.local");
    let content = std::fs::read_to_string(file).unwrap_or_else(|_| {
        eprintln!("Error: file not found: {file}");
        std::process::exit(1);
    });

    let config = Config::load().expect("failed to load config");
    let mut count = 0;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let val = val.trim();
            if !key.is_empty()
                && !val.is_empty()
                && keychain::Keychain::set(project, key, val).is_ok()
            {
                let _ = config.add_key(project, key);
                count += 1;
            }
        }
    }
    eprintln!("Pushed {count} secrets from {file} into Keychain for '{project}'");
}

fn cli_run(args: &[String]) {
    if args.is_empty() || args.len() < 2 {
        eprintln!("Usage: ke run <project> -- <command> [args...]");
        std::process::exit(1);
    }
    let project = &args[0];
    if let Err(msg) = validate_project_name(project) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }
    let cmd_args: Vec<&str> = args[1..].iter().map(|s| s.as_str()).collect();
    let cmd_start = if cmd_args.first() == Some(&"--") {
        1
    } else {
        0
    };

    if cmd_start >= cmd_args.len() {
        eprintln!("Error: no command specified");
        std::process::exit(1);
    }

    let config = Config::load().expect("failed to load config");
    let keys = config.list_keys(project).unwrap_or_default();

    let mut cmd = std::process::Command::new(cmd_args[cmd_start]);
    for arg in &cmd_args[cmd_start + 1..] {
        cmd.arg(arg);
    }

    // Set env vars
    for k in &keys {
        if let Some(v) = keychain::Keychain::get(project, k) {
            cmd.env(k, v);
        }
    }

    let status = cmd.status().unwrap_or_else(|e| {
        eprintln!("Error running command: {e}");
        std::process::exit(1);
    });

    std::process::exit(status.code().unwrap_or(1));
}

fn cli_rm_project(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: ke rm <project>");
        std::process::exit(1);
    }
    let project = &args[0];
    if let Err(msg) = validate_project_name(project) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }
    let config = Config::load().expect("failed to load config");
    let keys = config.list_keys(project).unwrap_or_default();
    for k in &keys {
        let _ = keychain::Keychain::delete(project, k);
    }
    let _ = config.remove_project(project);
    eprintln!("Removed project '{project}'");
}

// ═══════════════════════════════════════════════════════════════════════
// New: init, status
// ═══════════════════════════════════════════════════════════════════════

fn cli_init(args: &[String]) {
    let flag = args.first().map(|s| s.as_str()).unwrap_or("");
    if flag != "--icloud" {
        eprintln!("Usage: ke init --icloud");
        eprintln!("  Sets up iCloud Drive sync for project key names.");
        eprintln!("  Secret values stay local on each Mac's Keychain.");
        std::process::exit(1);
    }

    let mut config = Config::load().expect("failed to load config");
    if config.is_icloud_synced() {
        eprintln!("Already synced via iCloud Drive.");
        return;
    }
    match config.enable_icloud_sync() {
        Ok(()) => {
            eprintln!("✓ iCloud Drive sync enabled.");
            eprintln!("  Key names will sync to all your Macs via iCloud.");
            eprintln!("  On each Mac, run 'ke status' to see which values need to be set.");
        }
        Err(e) => {
            eprintln!("Error: {e}");
            eprintln!("  Make sure iCloud Drive is enabled in System Settings.");
            std::process::exit(1);
        }
    }
}

fn cli_status() {
    let config = Config::load().expect("failed to load config");
    let synced = config.is_icloud_synced();

    println!("ke status");
    println!("{}", "-".repeat(40));

    if synced {
        println!("  iCloud Drive: ✓ synced");
        if let Some(ref p) = config.icloud_dir {
            println!("  Location:     {}", p.display());
        }
    } else {
        println!("  iCloud Drive: not set up");
        println!("  Run 'ke init --icloud' to enable.");
    }
    println!("  Config:       {}", config.index_dir.display());
    println!();

    let keychain = RealKeychain::new();
    let statuses = config.status(&keychain).unwrap_or_default();
    if statuses.is_empty() {
        println!("No projects.")
    } else {
        println!("{:<20} {:>8} {:>12}", "Project", "Keys", "Synced here");
        println!("{}", "-".repeat(42));
        for (project, total, have) in &statuses {
            let missing = total - have;
            let status = if *have == *total {
                "✓ all".to_string()
            } else if *have == 0 {
                format!("✗ {} missing", missing)
            } else {
                format!("~ {} missing", missing)
            };
            println!("  {:<18} {:>6} {:>12}", project, total, status);
        }
    }
}
