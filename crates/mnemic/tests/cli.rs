use assert_cmd::Command;
use std::process::Output;

fn output(args: &[&str]) -> Output {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut command = Command::cargo_bin("mnemic").expect("mnemic binary");
    command.current_dir(dir.path());
    command.args(args).output().expect("run mnemic")
}

#[test]
fn help_exits_zero_and_shows_usage() {
    let output = output(&["--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert!(stdout.contains("Usage:"));
    insta::assert_snapshot!(stdout);
}

#[test]
fn version_prints_0_1_0() {
    let long = output(&["--version"]);
    let short = output(&["-V"]);
    assert!(long.status.success());
    assert!(short.status.success());
    assert_eq!(long.stdout, b"mnemic 0.1.0\n");
    assert_eq!(short.stdout, long.stdout);
}

#[test]
fn unknown_flag_exits_2() {
    let output = output(&["--not-a-flag"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn mcp_is_not_a_command() {
    let output = output(&["mcp", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("unrecognized subcommand 'mcp'"),
        "stderr was {stderr}"
    );
}
