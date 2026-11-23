mod common;

use std::process::Output;

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn explain_tool_exits_zero() {
    let out: Output = common::run_outside_a_workspace(&["explain", "tool"]);
    assert!(
        out.status.success(),
        "explain must know the tool role; stderr: {}",
        stderr(&out)
    );
}

#[test]
fn explain_tool_prints_the_contract() {
    let out: Output = common::run_outside_a_workspace(&["explain", "tool"]);
    let text: String = stdout(&out);
    assert!(
        text.contains("hex-lint — role `tool`"),
        "explain must name the role: {text}"
    );
    assert!(
        text.contains("May depend on: tool\n"),
        "a tool's row is exactly [tool]: {text}"
    );
    assert!(
        text.contains("no part of the running system"),
        "the contract must say what a tool is: {text}"
    );
}

#[test]
fn explain_tool_offers_no_context_exemption() {
    let out: Output = common::run_outside_a_workspace(&["explain", "tool"]);
    let text: String = stdout(&out);
    assert!(
        !text.contains("Context isolation:"),
        "a tool is judged on the context axis like any crate: {text}"
    );
    assert!(
        !text.contains("is exempt"),
        "a tool is judged on the context axis like any crate: {text}"
    );
}

#[test]
fn help_lists_tool_among_the_roles() {
    let out: Output = common::run_outside_a_workspace(&["--help"]);
    let text: String = stdout(&out);
    assert!(
        text.contains("    kernel, domain, usecase, port-and-adapter, driven-adapter, driving-adapter, infra, composition-root, tool\n"),
        "the ROLES line must name tool: {text}"
    );
}

#[test]
fn a_tool_depending_on_domain_fails_naming_the_edge() {
    let out: Output = common::run("tool_depends_on_domain", &[]);
    let err: String = stderr(&out);
    assert!(!out.status.success(), "a tool may not link domain: {err}");
    assert!(
        err.contains("gates (tool) -> core (domain): forbidden"),
        "expected the offending edge: {err}"
    );
}

#[test]
fn a_tool_depending_on_kernel_fails_naming_the_edge() {
    let out: Output = common::run("tool_depends_on_kernel", &[]);
    let err: String = stderr(&out);
    assert!(!out.status.success(), "a tool may not link kernel: {err}");
    assert!(
        err.contains("gates (tool) -> vocab (kernel): forbidden"),
        "expected the offending edge: {err}"
    );
}

#[test]
fn a_tool_with_no_workspace_dependency_passes() {
    let out: Output = common::run("tool_without_deps", &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("hex-lint: clean (2 workspace packages"),
        "stdout: {}",
        stdout(&out)
    );
}

#[test]
fn a_tool_depending_on_one_tool_passes() {
    let out: Output = common::run("tool_depends_on_one_tool", &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("hex-lint: clean (2 workspace packages"),
        "stdout: {}",
        stdout(&out)
    );
}

#[test]
fn a_tool_depending_on_two_tools_passes() {
    let out: Output = common::run("tool_depends_on_two_tools", &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("hex-lint: clean (3 workspace packages"),
        "stdout: {}",
        stdout(&out)
    );
}

#[test]
fn a_composition_root_depending_on_a_tool_fails_naming_the_edge() {
    let out: Output = common::run("composition_root_depends_on_tool", &[]);
    let err: String = stderr(&out);
    assert!(!out.status.success(), "a root may not link a tool: {err}");
    assert!(
        err.contains("app-wiring (composition-root) -> gates (tool): forbidden"),
        "expected the offending edge: {err}"
    );
}

#[test]
fn a_composition_root_depending_on_a_tool_is_told_why_and_how_to_fix_it() {
    let out: Output = common::run("composition_root_depends_on_tool", &[]);
    let err: String = stderr(&out);
    assert!(
        err.contains("why: the composition-root is where wiring lives"),
        "expected the root's rationale: {err}"
    );
    assert!(
        err.contains("A tool is no part of that system"),
        "the rationale must name what the root reached for: {err}"
    );
    assert!(
        err.contains("fix: If the root needs what a tool does"),
        "expected a fix for a root that reached for a tool: {err}"
    );
    assert!(
        !err.contains("file a bug"),
        "a root reaching for a tool is a real violation, not a tool bug: {err}"
    );
}

#[test]
fn a_tool_depending_on_a_tool_in_another_context_fails_naming_the_edge() {
    let out: Output = common::run("tool_crosses_contexts", &[]);
    let err: String = stderr(&out);
    assert!(!out.status.success(), "stderr: {err}");
    assert!(
        err.contains("gates [gates] -> views [views]: forbidden"),
        "expected the cross-context edge with both contexts: {err}"
    );
}

#[test]
fn a_tool_depending_on_a_tool_in_another_context_leaves_the_role_axis_clean() {
    let out: Output = common::run("tool_crosses_contexts", &[]);
    let err: String = stderr(&out);
    assert!(
        !err.contains("unsanctioned hex-arch role violations"),
        "tool -> tool is legal on the role axis: {err}"
    );
}

#[test]
fn a_tool_depending_on_a_tool_in_its_own_context_is_clean() {
    let out: Output = common::run("tool_within_one_context", &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("hex-lint: context isolation clean ("),
        "stdout: {}",
        stdout(&out)
    );
}

#[test]
fn a_tool_depending_on_a_tool_in_shared_is_clean() {
    let out: Output = common::run("tool_on_shared_tool", &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("hex-lint: context isolation clean ("),
        "stdout: {}",
        stdout(&out)
    );
}

#[test]
fn a_tool_without_a_context_under_adoption_is_the_partial_adoption_error() {
    let out: Output = common::run("tool_lacks_context", &[]);
    let err: String = stderr(&out);
    assert!(!out.status.success(), "stderr: {err}");
    assert!(
        err.contains("partial adoption is a hard error"),
        "expected the partial-adoption error: {err}"
    );
    assert!(
        err.contains("these lack context:\n  gates\n"),
        "the error must name the tool: {err}"
    );
}
