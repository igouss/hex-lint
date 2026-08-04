//! Acceptance — an exceptions entry naming a crate that isn't there says so.
//!
//! Both an entry whose debt was paid off and an entry whose crate names never
//! resolved are unmatched, and both fail the lint. They used to be reported
//! identically, under "no longer match a real violation / remove them — debt
//! paid off?". For a typo that reading is actively wrong twice over: nothing
//! was paid off, and the violation the entry meant to sanction may still be
//! there under the right name. Deleting the entry, as the message advised,
//! would then turn a sanctioned edge into a surprise build failure later.

mod common;

use std::process::Output;

fn stderr_of(extra: &[&str]) -> String {
    let out: Output = common::run("unknown_crate_exception", extra);
    String::from_utf8(out.stderr).expect("stderr is UTF-8")
}

#[test]
fn an_entry_naming_a_missing_crate_fails_the_run() {
    let out: Output = common::run("unknown_crate_exception", &[]);
    assert!(
        !out.status.success(),
        "an exceptions entry naming a crate that is not in the workspace must fail"
    );
}

#[test]
fn the_missing_crates_are_named() {
    let stderr: String = stderr_of(&[]);
    assert!(
        stderr.contains("ghost-crate") && stderr.contains("phantom-crate"),
        "both unresolvable names must be reported, got: {stderr}"
    );
}

#[test]
fn it_is_not_reported_as_paid_off_debt() {
    let stderr: String = stderr_of(&[]);
    assert!(
        !stderr.contains("debt paid off"),
        "a name that never resolved is not paid-off debt, got: {stderr}"
    );
}

#[test]
fn the_correctly_sanctioned_violation_is_not_reported() {
    // `dom -> db` matches a real violation, so it is doing its job and must
    // stay out of both unmatched buckets. Otherwise this fixture could pass
    // for the wrong reason.
    let stderr: String = stderr_of(&[]);
    assert!(
        !stderr.contains("ARCH-1"),
        "a matched exception must not be reported at all, got: {stderr}"
    );
}

#[test]
fn json_holds_the_entry_apart_from_stale_debt() {
    let out: Output = common::run("unknown_crate_exception", &["--format=json"]);
    let stdout: String = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    let doc: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

    let unknown: &Vec<serde_json::Value> = doc["unknown_crate_exceptions"]
        .as_array()
        .expect("unknown_crate_exceptions is an array");
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0]["consumer"], "ghost-crate");
    assert_eq!(unknown[0]["dep"], "phantom-crate");

    assert!(
        doc["stale_exceptions"]
            .as_array()
            .expect("stale_exceptions is an array")
            .is_empty(),
        "nothing here is stale debt"
    );
}

#[test]
fn the_exceptions_rule_reports_the_failure_it_caused() {
    // The report must not show `exceptions-honest` passing beside an overall
    // "fail" — a contradiction on the surface an agent parses.
    let out: Output = common::run("unknown_crate_exception", &["--format=json"]);
    let stdout: String = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    let doc: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

    assert_eq!(doc["status"], "fail");
    let rule: &serde_json::Value = doc["rules"]
        .as_array()
        .expect("rules is an array")
        .iter()
        .find(|r: &&serde_json::Value| r["id"] == "exceptions-honest")
        .expect("exceptions-honest is published");
    assert_eq!(rule["status"], "fail");
    assert_eq!(rule["failure_count"], 1);
}
