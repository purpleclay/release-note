use std::collections::HashMap;
use std::process::Command;

fn version_info() -> HashMap<String, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_release-note"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());

    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect()
}

// Returns None when built from a source archive without git metadata
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .env("TZ", "UTC")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).unwrap().trim().to_string())
}

#[test]
fn reports_the_version_without_the_build_date() {
    let info = version_info();

    assert_eq!(info["version"], env!("CARGO_PKG_VERSION"));
    assert!(!info.contains_key("build_date"));
}

#[test]
fn reports_the_commit_being_built() {
    let Some(commit) = git(&["rev-parse", "HEAD"]) else {
        return;
    };

    assert_eq!(version_info()["git_commit"], commit);
}

#[test]
fn reports_the_commit_date_in_utc() {
    let Some(date) = git(&[
        "log",
        "-1",
        "--format=%cd",
        "--date=format-local:%Y-%m-%dT%H:%M:%SZ",
    ]) else {
        return;
    };

    assert_eq!(version_info()["commit_date"], date);
}
