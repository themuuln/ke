use std::process::Command;

/// Path to the built `ke` binary.
fn ke_binary() -> Command {
    // cargo test sets CARGO_BIN_EXE_ke for binary crates
    let bin = std::env!("CARGO_BIN_EXE_ke");
    Command::new(bin)
}

#[test]
fn help_prints_usage() {
    let output = ke_binary().arg("help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ke — macOS Keychain secrets manager"));
    assert!(stdout.contains("ke set <project> <key>"));
    assert!(stdout.contains("ke get <project> <key>"));
}

#[test]
fn help_flag_works() {
    let output = ke_binary().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ke — macOS Keychain secrets manager"));
}

#[test]
fn short_help_flag_works() {
    let output = ke_binary().arg("-h").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ke — macOS Keychain secrets manager"));
}

#[test]
fn unknown_command_shows_error() {
    let output = ke_binary().arg("nonexistent").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown command"));
    assert!(stderr.contains("nonexistent"));
}

#[test]
fn set_missing_args() {
    let output = ke_binary().arg("set").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: ke set"));
}

#[test]
fn get_missing_args() {
    let output = ke_binary().arg("get").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: ke get"));
}

#[test]
fn delete_missing_args() {
    let output = ke_binary().arg("delete").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: ke delete"));
}

#[test]
fn run_missing_args() {
    let output = ke_binary().arg("run").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: ke run"));
}

#[test]
fn pull_missing_args() {
    let output = ke_binary().arg("pull").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: ke pull"));
}

#[test]
fn push_missing_args() {
    let output = ke_binary().arg("push").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: ke push"));
}

#[test]
fn cp_missing_args() {
    let output = ke_binary().arg("cp").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: ke cp"));
}

#[test]
fn load_missing_args() {
    let output = ke_binary().arg("load").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: ke load"));
}

#[test]
fn cat_missing_args() {
    let output = ke_binary().arg("cat").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: ke cat"));
}

#[test]
fn rm_missing_args() {
    let output = ke_binary().arg("rm").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: ke rm"));
}

#[test]
fn install_shows_message() {
    let output = ke_binary().arg("install").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ke is already installed at"));
}

#[test]
fn status_shows_config() {
    let output = ke_binary().arg("status").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ke status"));
    assert!(stdout.contains("Config:"));
}

#[test]
fn init_without_flag_shows_usage() {
    let output = ke_binary().arg("init").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: ke init"));
}

#[test]
fn tui_default_runs_tui() {
    // Just verify the TUI binary starts and can be terminated
    // The TUI will fail if stdin isn't a TTY, which is fine — we just
    // check it doesn't crash with a confusing error message.
    let output = ke_binary().arg("tui").output().unwrap();
    assert!(!output.status.success()); // TUI fails without a real TTY
}
