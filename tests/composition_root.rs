//! Process-level proof of the context axis's exemption for a composition root.
//!
//! A composition root sits outside every hexagon: its whole job is to wire
//! contexts together, so the context axis does not judge an edge out of one.
//! Two fixtures carry the pair of claims that make that an exemption rather
//! than a loosening — a root reaching into two contexts is clean, and a crate
//! reaching into a root is not — and these tests read only the binary's
//! observable output.

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

// --- composition_root_wires_contexts: a root names a usecase in `shopping`
//     and a usecase in `pantry`. Both axes clean.

#[test]
fn a_root_wiring_two_contexts_exits_zero() {
    let out: Output = common::run("composition_root_wires_contexts", &[]);
    assert!(
        out.status.success(),
        "a composition root may wire every context; stderr: {}",
        stderr(&out)
    );
}

#[test]
fn a_root_wiring_two_contexts_is_clean_on_both_axes() {
    let out: Output = common::run("composition_root_wires_contexts", &[]);
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

#[test]
fn a_root_wiring_two_contexts_reports_no_context_violation() {
    let out: Output = common::run("composition_root_wires_contexts", &[]);
    let err: String = stderr(&out);
    assert!(
        !err.contains("context-isolation violations"),
        "the context axis must not judge an edge out of a root: {err}"
    );
}

// --- depends_on_composition_root: the exemption is not a back door. One
//     context, so the context axis is clean; the role matrix still refuses.

#[test]
fn depending_on_a_root_exits_non_zero() {
    let out: Output = common::run("depends_on_composition_root", &[]);
    assert!(!out.status.success(), "nothing may depend on a root");
}

#[test]
fn depending_on_a_root_fails_on_the_role_axis() {
    let out: Output = common::run("depends_on_composition_root", &[]);
    let err: String = stderr(&out);
    assert!(
        err.contains("unsanctioned hex-arch role violations"),
        "expected a role-axis failure: {err}"
    );
    assert!(
        err.contains("app-shell (usecase) -> app-wiring (composition-root)"),
        "expected the offending edge: {err}"
    );
}

#[test]
fn depending_on_a_root_leaves_the_context_axis_clean() {
    let out: Output = common::run("depends_on_composition_root", &[]);
    let err: String = stderr(&out);
    assert!(
        !err.contains("context-isolation violations"),
        "both crates share a context; only the role axis may fire: {err}"
    );
}

// --- the rule is stated where the tool states rules.

#[test]
fn explain_tells_a_root_it_is_exempt() {
    let out: Output = common::run_outside_a_workspace(&["explain", "composition-root"]);
    let text: String = stdout(&out);
    assert!(
        text.contains("Context isolation:"),
        "explain must state the context axis for a root: {text}"
    );
    assert!(
        text.contains("is exempt"),
        "explain must state the exemption: {text}"
    );
}

#[test]
fn explain_does_not_offer_the_exemption_to_a_role_that_lacks_it() {
    let out: Output = common::run_outside_a_workspace(&["explain", "usecase"]);
    let text: String = stdout(&out);
    assert!(
        !text.contains("Context isolation:"),
        "a usecase stands inside its context: {text}"
    );
}
