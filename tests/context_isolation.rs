//! Acceptance #2–#7 — process-level proof of the context-isolation axis.
//!
//! Each fixture under `tests/fixtures/` is a self-contained virtual workspace
//! mapped 1:1 to an acceptance item. These tests run the compiled binary
//! against each and assert on its observable behaviour (exit status plus
//! stdout/stderr substrings) — no JSON parser, no new dependencies. The two
//! axes are kept orthogonal here just as they are in the source: a role-axis
//! assertion never leans on context state and vice versa.

mod common;

use std::process::Output;

/// The binary's stdout as text (lossy is fine — every message is ASCII/UTF-8).
fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The binary's stderr as text.
fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// --- #2 clean_contexts: all contexts declared, every edge legal on both axes.

#[test]
fn clean_contexts_exits_zero() {
    let out: Output = common::run("clean_contexts", &[]);
    assert!(out.status.success(), "clean_contexts must exit 0");
}

#[test]
fn clean_contexts_prints_both_clean_summary_lines() {
    let out: Output = common::run("clean_contexts", &[]);
    let text: String = stdout(&out);
    assert!(
        text.contains("hex-lint: clean ("),
        "role summary line missing: {text}"
    );
    assert!(
        text.contains("hex-lint: context isolation clean ("),
        "context summary line missing: {text}"
    );
}

// --- #3 cross_context (load-bearing): role legal, context illegal.

#[test]
fn cross_context_exits_non_zero() {
    let out: Output = common::run("cross_context", &[]);
    assert!(!out.status.success(), "cross_context must fail");
}

#[test]
fn cross_context_role_axis_passes() {
    // driving-adapter -> usecase is role-legal, so no role-violation header
    // ever reaches stderr.
    let out: Output = common::run("cross_context", &[]);
    let err: String = stderr(&out);
    assert!(
        !err.contains("hex-arch role violations"),
        "role axis must stay clean: {err}"
    );
}

#[test]
fn cross_context_context_axis_fails() {
    let out: Output = common::run("cross_context", &[]);
    let err: String = stderr(&out);
    assert!(
        err.contains("unsanctioned context-isolation violations"),
        "expected a context-axis failure: {err}"
    );
    assert!(
        err.contains("shopping-reactor [shopping] -> pantry-shell [pantry]"),
        "expected the offending cross-context edge: {err}"
    );
}

#[test]
fn cross_context_json_names_the_context_axis() {
    let out: Output = common::run("cross_context", &["--format=json"]);
    let json: String = stdout(&out);
    assert!(
        json.contains("\"axis\": \"context\""),
        "json must attribute the failure to the context axis: {json}"
    );
    assert!(
        !json.contains("\"axis\": \"role\""),
        "no role violation should be emitted: {json}"
    );
}

// --- #4 shared_ok: a shopping crate depends on a shared crate; both axes pass.

#[test]
fn shared_ok_exits_zero() {
    let out: Output = common::run("shared_ok", &[]);
    assert!(out.status.success(), "shared_ok must exit 0");
}

#[test]
fn shared_ok_is_clean_on_both_axes() {
    let out: Output = common::run("shared_ok", &[]);
    let text: String = stdout(&out);
    assert!(
        text.contains("hex-lint: clean ("),
        "role summary line missing: {text}"
    );
    assert!(
        text.contains("hex-lint: context isolation clean ("),
        "context summary line missing: {text}"
    );
}

// --- #5 shared_reaches_in: shared -> non-shared. Role legal, context illegal.

#[test]
fn shared_reaches_in_exits_non_zero() {
    let out: Output = common::run("shared_reaches_in", &[]);
    assert!(!out.status.success(), "shared_reaches_in must fail");
}

#[test]
fn shared_reaches_in_fails_on_context_not_role() {
    let out: Output = common::run("shared_reaches_in", &[]);
    let err: String = stderr(&out);
    assert!(
        err.contains("unsanctioned context-isolation violations"),
        "expected a context-axis failure: {err}"
    );
    assert!(
        err.contains("contracts [shared] -> pantry-core [pantry]"),
        "expected the shared-reaches-in edge: {err}"
    );
    assert!(
        !err.contains("hex-arch role violations"),
        "role axis must stay clean: {err}"
    );
}

// --- #6 partial_context: one member omits context -> hard error naming it.

#[test]
fn partial_context_exits_non_zero() {
    let out: Output = common::run("partial_context", &[]);
    assert!(
        !out.status.success(),
        "partial adoption must be a hard error"
    );
}

#[test]
fn partial_context_hard_errors_naming_the_uncovered_crate() {
    let out: Output = common::run("partial_context", &[]);
    let err: String = stderr(&out);
    assert!(
        err.contains("partial adoption is a hard error"),
        "expected the partial-adoption abort: {err}"
    );
    assert!(
        err.contains("unlabeled"),
        "must name the crate lacking a context: {err}"
    );
}

// --- #7a context_exception_suppresses: a context-axis exception clears exactly
//     its edge; a role-axis exception for the same edge does NOT.

#[test]
fn context_exception_suppresses_makes_the_run_clean() {
    // The fixture's committed hex-lint-exceptions.toml sanctions the sole
    // cross-context edge on the context axis.
    let out: Output = common::run("context_exception_suppresses", &[]);
    assert!(
        out.status.success(),
        "a context-axis exception should sanction the edge"
    );
    let text: String = stdout(&out);
    assert!(
        text.contains("hex-lint: context isolation clean ("),
        "expected a clean context summary: {text}"
    );
}

#[test]
fn role_axis_exception_does_not_suppress_a_context_violation() {
    // Same edge, exception routed to the role axis: per-axis routing means the
    // context check ignores it and still fails.
    let exceptions: String = format!(
        "{}/tests/fixtures/context_exception_suppresses/role-exception.toml",
        env!("CARGO_MANIFEST_DIR")
    );
    let out: Output = common::run(
        "context_exception_suppresses",
        &["--exceptions", exceptions.as_str()],
    );
    assert!(
        !out.status.success(),
        "a role-axis exception must not clear a context violation"
    );
    let err: String = stderr(&out);
    assert!(
        err.contains("shopping-reactor [shopping] -> pantry-shell [pantry]"),
        "the context violation must still be reported: {err}"
    );
}

// --- #7b context_exception_stale: a context-axis exception matching no edge.

#[test]
fn stale_context_exception_exits_non_zero() {
    let out: Output = common::run("context_exception_stale", &[]);
    assert!(!out.status.success(), "a stale context exception must fail");
}

#[test]
fn stale_context_exception_is_reported_as_debt() {
    let out: Output = common::run("context_exception_stale", &[]);
    let err: String = stderr(&out);
    assert!(
        err.contains("context-axis exceptions file entries that no longer match a real violation"),
        "expected the stale-exception report: {err}"
    );
    assert!(
        err.contains("shopping-usecase -> ghost-crate"),
        "expected the stale entry: {err}"
    );
}
