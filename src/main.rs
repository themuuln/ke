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
use crate::keychain::{KeychainBackend, RealKeychain};

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
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err("key name cannot be empty".into());
    };
    if !(first.is_ascii_uppercase() || first == '_') {
        return Err("key name must start with an uppercase ASCII letter or underscore".into());
    }
    if !chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') {
        return Err("key name must be uppercase ASCII alphanumeric or underscore".into());
    }
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn dotenv_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

fn parse_dotenv_value(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    let trimmed = raw.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let mut parsed = String::new();
        let mut chars = trimmed[1..trimmed.len() - 1].chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.next() {
                    Some('n') => parsed.push('\n'),
                    Some('r') => parsed.push('\r'),
                    Some('t') => parsed.push('\t'),
                    Some('\\') => parsed.push('\\'),
                    Some('"') => parsed.push('"'),
                    Some(other) => parsed.push(other),
                    None => parsed.push('\\'),
                }
            } else {
                parsed.push(ch);
            }
        }
        Some(parsed)
    } else if trimmed.len() >= 2 && trimmed.starts_with('\'') && trimmed.ends_with('\'') {
        Some(trimmed[1..trimmed.len() - 1].to_string())
    } else {
        Some(raw.to_string())
    }
}

fn project_values(
    config: &Config,
    keychain: &dyn KeychainBackend,
    project: &str,
) -> Vec<(String, String)> {
    let keys = config.list_keys(project).unwrap_or_default();
    keychain.list_values(project, &keys)
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
    let config = Config::load()?;
    let mut app = App::new(config, Arc::new(RealKeychain::new()))?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(e) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(e.into());
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(e) => {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            return Err(e.into());
        }
    };
    let res = terminal
        .clear()
        .map_err(anyhow::Error::from)
        .and_then(|_| run_app(&mut terminal, &mut app));

    let mut cleanup_err: Option<anyhow::Error> = None;
    if let Err(e) = disable_raw_mode() {
        cleanup_err = Some(e.into());
    }
    if let Err(e) = execute!(terminal.backend_mut(), LeaveAlternateScreen) {
        cleanup_err.get_or_insert_with(|| e.into());
    }
    if let Err(e) = terminal.show_cursor() {
        cleanup_err.get_or_insert_with(|| e.into());
    }

    if let Some(cleanup_err) = cleanup_err {
        if let Err(e) = res {
            eprintln!("Error: {:#}", e);
        }
        return Err(cleanup_err);
    }
    res
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    terminal.draw(|frame| ui::draw(frame, app))?;
    loop {
        let (running, redraw) = app.handle_event()?;
        if !running {
            break;
        }
        if redraw {
            terminal.draw(|frame| ui::draw(frame, app))?;
        }
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
    let keychain = RealKeychain::new();
    println!("# {project} — from macOS Keychain");
    for (key, value) in project_values(&config, &keychain, project) {
        println!("export {key}={}", shell_quote(&value));
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
    let keychain = RealKeychain::new();
    println!("# {project} — from macOS Keychain");
    for (key, value) in project_values(&config, &keychain, project) {
        println!("{key}={}", dotenv_quote(&value));
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
    let delete_res = keychain::Keychain::delete(project, key);
    match delete_res {
        Ok(()) => {
            let config = Config::load().expect("failed to load config");
            let _ = config.remove_key(project, key);
            eprintln!("Deleted {project}:{key}");
        }
        Err(e) if keychain::is_not_found_error(&e) => {
            let config = Config::load().expect("failed to load config");
            let _ = config.remove_key(project, key);
            eprintln!("Removed {project}:{key} from index");
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
    let keychain = RealKeychain::new();
    let out = std::env::current_dir()
        .expect("failed to get current dir")
        .join(".env.local");
    let mut content = format!("# {project} — pulled from Keychain\n");
    content.push_str("# WARNING: contains secrets — never commit!\n\n");
    let mut count = 0;
    for (key, value) in project_values(&config, &keychain, project) {
        content.push_str(&format!("{key}={}\n", dotenv_quote(&value)));
        count += 1;
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
    let keychain = RealKeychain::new();
    let mut count = 0;
    let mut saved_keys = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let Some(val) = parse_dotenv_value(val) else {
                continue;
            };
            if validate_key_name(key).is_err() || val.is_empty() {
                continue;
            }
            if keychain.set(project, key, &val).is_ok() {
                saved_keys.push(key.to_string());
                count += 1;
            }
        }
    }
    let _ = config.add_keys(project, &saved_keys);
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
    let keychain = RealKeychain::new();

    let mut cmd = std::process::Command::new(cmd_args[cmd_start]);
    for arg in &cmd_args[cmd_start + 1..] {
        cmd.arg(arg);
    }

    // Set env vars
    for (key, value) in project_values(&config, &keychain, project) {
        cmd.env(key, value);
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
    let mut failed = 0;
    for k in &keys {
        if let Err(e) = keychain::Keychain::delete(project, k) {
            if !keychain::is_not_found_error(&e) {
                failed += 1;
            }
        }
    }
    if failed > 0 {
        eprintln!("Error: failed to delete {failed} secret(s) from Keychain");
        std::process::exit(1);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names_must_be_valid_env_names() {
        assert!(validate_key_name("API_KEY_1").is_ok());
        assert!(validate_key_name("_TOKEN").is_ok());
        assert!(validate_key_name("1BAD").is_err());
        assert!(validate_key_name("lower").is_err());
        assert!(validate_key_name("BAD-NAME").is_err());
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("abc"), "'abc'");
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn dotenv_quote_escapes_special_chars() {
        assert_eq!(dotenv_quote("a b"), "\"a b\"");
        assert_eq!(dotenv_quote("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }

    #[test]
    fn parse_dotenv_value_decodes_quoted_values() {
        assert_eq!(
            parse_dotenv_value("\"a\\\"b\\\\c\\n\""),
            Some("a\"b\\c\n".into())
        );
        assert_eq!(parse_dotenv_value(" leading "), Some(" leading ".into()));
    }
}
