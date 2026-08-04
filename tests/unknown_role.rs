//! Acceptance — a bad role tag is told what the roles are.
//!
//! A role is the one tag every workspace member must carry, and the first
//! thing anyone adopting hex-lint writes. `Role::parse` is exact-match, so
//! `Domain`, `use-case` and `ports` all fail. Being told only that the value
//! is wrong sends the reader off to find a list the tool is already holding.

mod common;

use std::process::Output;

/// Every role name hex-lint accepts, as `--capabilities` publishes them. Read
/// from the binary rather than repeated here: a hard-coded list in this file
/// would be the very drift the assertion exists to catch.
fn every_role_name() -> Vec<String> {
    let out: Output = common::run_outside_a_workspace(&["--capabilities"]);
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("--capabilities emits JSON");
    doc["matrix"]
        .as_array()
        .expect("matrix is an array")
        .iter()
        .map(|row: &serde_json::Value| {
            row["consumer_role"]
                .as_str()
                .expect("consumer_role is a string")
                .to_owned()
        })
        .collect()
}

#[test]
fn an_unknown_role_tag_fails_the_run() {
    let out: Output = common::run("unknown_role", &[]);
    assert!(!out.status.success(), "a bad role tag must fail the run");
}

#[test]
fn the_diagnostic_quotes_the_offending_value_and_names_the_crate() {
    let out: Output = common::run("unknown_role", &[]);
    let stderr: String = String::from_utf8(out.stderr).expect("stderr is UTF-8");
    assert!(
        stderr.contains("mistagged") && stderr.contains("`Domain`"),
        "the diagnostic must name the crate and quote the bad value, got: {stderr}"
    );
}

#[test]
fn the_diagnostic_offers_every_valid_role() {
    let out: Output = common::run("unknown_role", &[]);
    let stderr: String = String::from_utf8(out.stderr).expect("stderr is UTF-8");
    for role in every_role_name() {
        assert!(
            stderr.contains(&role),
            "the diagnostic must offer `{role}`, got: {stderr}"
        );
    }
}
