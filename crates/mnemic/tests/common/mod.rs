use std::path::Path;
use std::sync::OnceLock;

use assert_cmd::Command;
use tempfile::TempDir;

static CONFIG_HOME: OnceLock<TempDir> = OnceLock::new();

fn config_dir() -> &'static TempDir {
    CONFIG_HOME.get_or_init(|| tempfile::tempdir().expect("xdg config home"))
}

pub fn config_home() -> &'static Path {
    config_dir().path()
}

/// `mnemic` with `XDG_CONFIG_HOME` pointed at an empty process-wide directory
/// and `HOME` removed, so a developer's real default vault cannot reach a test.
/// The `TempDir` is never dropped before an assertion.
pub fn mnemic() -> Command {
    let mut command = Command::cargo_bin("mnemic").expect("mnemic binary");
    command.env("XDG_CONFIG_HOME", config_home());
    command.env_remove("HOME");
    command
}
