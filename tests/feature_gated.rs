//! Acceptance — a forbidden edge behind a non-default feature is still caught.
//!
//! `cargo metadata` resolves the feature graph. Asked with default features, it
//! omits an optional dependency that no default feature activates, so a lint
//! reading that graph reports "clean" while the forbidden edge exists. That is
//! a false green: the gate says pass and ships the violation.
//!
//! The `feature_gated` fixture is exactly that shape — a `domain` crate with an
//! optional dependency on a `driven-adapter`, activated only by the
//! non-default `persistence` feature. These tests fail if the loader ever stops
//! asking for all features.

mod common;

use std::process::Output;

#[test]
fn optional_dependency_behind_a_non_default_feature_is_a_violation() {
    let out: Output = common::run("feature_gated", &[]);
    assert!(
        !out.status.success(),
        "a domain -> driven-adapter edge must fail even when it is feature-gated"
    );
}

#[test]
fn the_feature_gated_edge_is_named_in_the_diagnostic() {
    let out: Output = common::run("feature_gated", &[]);
    let stderr: String = String::from_utf8(out.stderr).expect("stderr is UTF-8");
    assert!(
        stderr.contains("pure-domain") && stderr.contains("db-adapter"),
        "the diagnostic must name both ends of the edge, got: {stderr}"
    );
}

#[test]
fn the_feature_gated_edge_is_reported_as_json_too() {
    let out: Output = common::run("feature_gated", &["--format=json"]);
    let stdout: String = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    let doc: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let violations: &Vec<serde_json::Value> = doc["violations"]
        .as_array()
        .expect("violations is an array");
    assert_eq!(violations.len(), 1, "expected exactly one violation");
    assert_eq!(violations[0]["consumer"], "pure-domain");
    assert_eq!(violations[0]["dep"], "db-adapter");
}
