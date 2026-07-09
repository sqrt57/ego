//! Integration tests for the `ego` binary's CLI: script-mode execution,
//! mixed `-e`/file fragments, exit codes, and `file:line:col:` diagnostics.
//! See cli.md.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_ego"))
}

/// Writes `contents` to a uniquely-named temp file and returns its path.
/// `tag` only needs to be unique per call site (tests run in parallel).
struct TempScript(PathBuf);

impl TempScript {
    fn new(tag: &str, contents: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!("ego_cli_test_{}_{}.ego", std::process::id(), tag));
        let mut f = std::fs::File::create(&path).expect("create temp script");
        f.write_all(contents.as_bytes()).expect("write temp script");
        Self(path)
    }

    fn path(&self) -> &str {
        self.0.to_str().expect("temp path is valid UTF-8")
    }
}

impl Drop for TempScript {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(bin()).args(args).output().expect("run ego binary")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn script_mode_does_not_auto_print() {
    let script = TempScript::new("bare_expr", "1 + 1");
    let out = run(&[script.path()]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out), "");
}

#[test]
fn script_error_reports_path_line_and_column() {
    let script = TempScript::new("runtime_error", "1.\n2.\n1 / 0");
    let out = run(&[script.path()]);
    assert_eq!(out.status.code(), Some(1));
    let expected = format!("{}:3:3: error: division by zero", script.path());
    assert!(
        stderr(&out).contains(&expected),
        "expected stderr to contain {expected:?}, got: {}",
        stderr(&out)
    );
}

#[test]
fn no_arguments_is_bad_args() {
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("Usage:"), "got: {}", stderr(&out));
}

#[test]
fn unknown_option_is_bad_args() {
    let out = run(&["--bogus"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn eval_only_mode_prints_final_result() {
    let out = run(&["-e", "3 + 4"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out), "7\n");
}

#[test]
fn mixed_eval_and_file_suppresses_auto_print() {
    // cli.md "Mixed eval and files": no result is printed for any fragment,
    // including `-e` ones, once a file is part of the invocation.
    let script = TempScript::new("mixed_file", "1");
    let out = run(&["-e", "3 + 4", script.path()]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out), "");
}

#[test]
fn multiple_files_run_in_order() {
    // `first_ok` succeeds; `second_error` fails. The reported error must
    // name `second_error`'s path, proving `first_ok` ran (and succeeded)
    // before it, in argument order.
    let first_ok = TempScript::new("order_first", "1");
    let second_error = TempScript::new("order_second", "1 / 0");
    let out = run(&[first_ok.path(), second_error.path()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains(second_error.path()),
        "got: {}",
        stderr(&out)
    );
}

#[test]
fn nonexistent_file_is_fatal() {
    let mut missing = std::env::temp_dir();
    missing.push(format!("ego_cli_test_{}_missing.ego", std::process::id()));
    let out = run(&[missing.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(!stderr(&out).is_empty());
}

#[test]
fn version_flag() {
    let out = run(&["--version"]);
    assert!(out.status.success());
    assert!(stdout(&out).starts_with("ego "), "got: {}", stdout(&out));
}

#[test]
fn help_flag() {
    let out = run(&["--help"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Usage:"), "got: {}", stdout(&out));
}
