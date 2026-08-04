//! Process-level contract for `--capabilities`.
//!
//! The unit tests in `src/capabilities.rs` prove the document is *derived* from
//! the enforcing types. These prove the two properties only the binary can
//! demonstrate: it answers with **no workspace**, and it answers **exit 0** —
//! the difference between a contract an agent can read up front and one it can
//! only obtain by successfully running a check.

mod common;

use serde_json::Value;

fn capabilities_document() -> Value {
    let out: std::process::Output = common::run_outside_a_workspace(&["--capabilities"]);
    assert!(
        out.status.success(),
        "--capabilities must exit 0 outside a workspace; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("--capabilities emits JSON on stdout")
}

#[test]
fn answers_outside_a_workspace_with_exit_zero() {
    let doc: Value = capabilities_document();
    assert_eq!(doc["tool"], "hex-lint");
    assert_eq!(doc["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn emits_nothing_on_stderr() {
    // stdout is the payload; an agent piping it into a parser must not have to
    // strip a banner, and a warning here would be indistinguishable from noise.
    let out: std::process::Output = common::run_outside_a_workspace(&["--capabilities"]);
    assert!(
        out.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn publishes_the_matrix_the_check_enforces() {
    // Cross-check the published contract against a real run's report on the
    // clean fixture: same tool, two surfaces, one matrix. This is the join the
    // unit tests cannot make — they never start the binary.
    let doc: Value = capabilities_document();
    let run: std::process::Output = common::run("no_context", &["--format=json"]);
    let report: Value = serde_json::from_slice(&run.stdout).expect("report is JSON");
    assert_eq!(
        doc["matrix"], report["matrix"],
        "the matrix in --capabilities and the matrix in the report have drifted"
    );
}

#[test]
fn publishes_the_rule_ids_the_report_returns() {
    let doc: Value = capabilities_document();
    let run: std::process::Output = common::run("no_context", &["--format=json"]);
    let report: Value = serde_json::from_slice(&run.stdout).expect("report is JSON");

    let published: Vec<&str> = doc["rules"]
        .as_array()
        .expect("rules is an array")
        .iter()
        .map(|r: &Value| r["id"].as_str().expect("id is a string"))
        .collect();
    let returned: Vec<&str> = report["rules"]
        .as_array()
        .expect("report rules is an array")
        .iter()
        .map(|r: &Value| r["id"].as_str().expect("id is a string"))
        .collect();
    assert_eq!(
        published, returned,
        "an agent keying on a published rule id would miss a returned one"
    );
}

#[test]
fn is_deterministic() {
    // Two invocations, byte-identical output: the document is a static value,
    // so it can be cached, diffed, and committed as a fixture by a consumer.
    let first: std::process::Output = common::run_outside_a_workspace(&["--capabilities"]);
    let second: std::process::Output = common::run_outside_a_workspace(&["--capabilities"]);
    assert_eq!(first.stdout, second.stdout);
}

#[test]
fn ignores_other_arguments() {
    // `--capabilities` short-circuits: it must not require, or be perturbed by,
    // the flags a check would need.
    let bare: std::process::Output = common::run_outside_a_workspace(&["--capabilities"]);
    let noisy: std::process::Output = common::run_outside_a_workspace(&[
        "--format=json",
        "--capabilities",
        "--manifest-path",
        "/nonexistent/Cargo.toml",
    ]);
    assert!(
        noisy.status.success(),
        "--capabilities must not load a workspace"
    );
    assert_eq!(bare.stdout, noisy.stdout);
}
