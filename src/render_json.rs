use serde::Serialize;

use crate::context;
use crate::context_check;
use crate::lint::{self, Axis, Exception};
use crate::remediation::Remediation;
use crate::role::Role;
use crate::role_check;
use crate::rule::Rule;

pub fn render_json(
    packages: &[lint::WorkspacePackage],
    role_report: &lint::AxisReport<role_check::RoleViolation>,
    context_report: &lint::AxisReport<context_check::ContextViolation>,
    exceptions: &[Exception],
    had_problem: bool,
) {
    // The exception->violation join is axis-aware: a role exception may only
    // annotate a role violation, a context exception only a context violation.
    let role_exc_by_key: std::collections::BTreeMap<(&str, &str), &Exception> = exceptions
        .iter()
        .filter(|e: &&Exception| e.axis == Axis::Role)
        .map(|e: &Exception| ((e.consumer.as_str(), e.dep.as_str()), e))
        .collect();
    let context_exc_by_key: std::collections::BTreeMap<(&str, &str), &Exception> = exceptions
        .iter()
        .filter(|e: &&Exception| e.axis == Axis::Context)
        .map(|e: &Exception| ((e.consumer.as_str(), e.dep.as_str()), e))
        .collect();

    let violations: Vec<JsonViolation<'_>> = role_report
        .violations
        .iter()
        .map(|v: &role_check::RoleViolation| {
            let exc: Option<&&Exception> =
                role_exc_by_key.get(&(v.consumer.as_str(), v.dep.as_str()));
            let rem: Remediation = v.consumer_role.remediation();
            JsonViolation {
                axis: Axis::Role,
                consumer: &v.consumer,
                consumer_role: v.consumer_role.as_str(),
                dep: &v.dep,
                dep_role: v.dep_role.as_str(),
                sanctioned: exc.is_some(),
                ticket: exc.map(|e: &&Exception| e.ticket.as_str()),
                reason: exc.map(|e: &&Exception| e.reason.as_str()),
                remediation: JsonRemediation {
                    rule: v.consumer_role.contract(),
                    fixes: rem.fixes,
                },
            }
        })
        .collect();

    let context_rem: Remediation = context::remediation();
    let context_violations: Vec<JsonContextViolation<'_>> = context_report
        .violations
        .iter()
        .map(|v: &context_check::ContextViolation| {
            let exc: Option<&&Exception> =
                context_exc_by_key.get(&(v.consumer.as_str(), v.dep.as_str()));
            JsonContextViolation {
                axis: Axis::Context,
                consumer: &v.consumer,
                consumer_context: &v.consumer_context,
                dep: &v.dep,
                dep_context: &v.dep_context,
                sanctioned: exc.is_some(),
                ticket: exc.map(|e: &&Exception| e.ticket.as_str()),
                reason: exc.map(|e: &&Exception| e.reason.as_str()),
                remediation: JsonRemediation {
                    rule: context_rem.rationale.to_owned(),
                    fixes: context_rem.fixes,
                },
            }
        })
        .collect();

    let stale_exceptions: Vec<JsonException<'_>> = role_report
        .stale_exceptions
        .iter()
        .map(JsonException::from_lint)
        .collect();

    let context_stale_exceptions: Vec<JsonException<'_>> = context_report
        .stale_exceptions
        .iter()
        .map(JsonException::from_lint)
        .collect();

    let unknown_crate_exceptions: Vec<JsonException<'_>> = role_report
        .unknown_crate_exceptions
        .iter()
        .map(JsonException::from_lint)
        .collect();

    let context_unknown_crate_exceptions: Vec<JsonException<'_>> = context_report
        .unknown_crate_exceptions
        .iter()
        .map(JsonException::from_lint)
        .collect();

    let exception_entries: Vec<JsonException<'_>> =
        exceptions.iter().map(JsonException::from_lint).collect();

    let json_packages: Vec<JsonPackage<'_>> = packages
        .iter()
        .map(|p: &lint::WorkspacePackage| JsonPackage {
            name: &p.name,
            role: p.role.as_str(),
            context: p.context.as_deref(),
        })
        .collect();

    let matrix: Vec<JsonMatrixRow> = ALL_ROLES
        .iter()
        .map(|&r: &Role| JsonMatrixRow {
            consumer_role: r.as_str(),
            allowed_deps: r.allowed_deps().iter().map(|d: &Role| d.as_str()).collect(),
        })
        .collect();

    // Ids and descriptions come from `Rule`, the same enumeration
    // `--capabilities` publishes, so the contract an agent read and the report
    // it gets back cannot disagree. Only the per-run counts live here.
    //
    // `exceptions-honest` spans BOTH axes: a stale context exception fails the
    // build via `had_problem`, so leaving it out of the count let `rules` report
    // all-pass beside `"status": "fail"` — a contradiction on the surface an
    // agent parses.
    let rules: Vec<JsonRule<'_>> = Rule::ALL
        .iter()
        .map(|&r: &Rule| {
            let failure_count: usize = match r {
                Rule::RoleMatrix => role_report.unsanctioned.len(),
                // Both ways an entry can be dishonest count here — stale, and
                // naming a crate that is not in the workspace. Either fails the
                // build via `had_problem`, so either must show up.
                Rule::ExceptionsHonest => {
                    role_report.stale_exceptions.len()
                        + context_report.stale_exceptions.len()
                        + role_report.unknown_crate_exceptions.len()
                        + context_report.unknown_crate_exceptions.len()
                }
                Rule::ContextIsolation => context_report.unsanctioned.len(),
            };
            JsonRule {
                id: r.id(),
                description: r.description(),
                status: if failure_count == 0 { "pass" } else { "fail" },
                failure_count,
            }
        })
        .collect();

    let out: JsonReport<'_> = JsonReport {
        version: 1,
        status: if had_problem { "fail" } else { "pass" },
        package_count: packages.len(),
        packages: json_packages,
        matrix,
        rules,
        violations,
        context_violations,
        exceptions: exception_entries,
        stale_exceptions,
        context_stale_exceptions,
        unknown_crate_exceptions,
        context_unknown_crate_exceptions,
    };

    match serde_json::to_string_pretty(&out) {
        Ok(s) => println!("{s}"),
        Err(e) => {
            // Fallback: never lose the failure signal.
            eprintln!("hex-lint: json serialization failed: {e}");
        }
    }
}

const ALL_ROLES: &[Role] = Role::ALL;

#[derive(Serialize)]
struct JsonReport<'a> {
    version: u32,
    status: &'a str,
    package_count: usize,
    packages: Vec<JsonPackage<'a>>,
    matrix: Vec<JsonMatrixRow<'a>>,
    rules: Vec<JsonRule<'a>>,
    violations: Vec<JsonViolation<'a>>,
    context_violations: Vec<JsonContextViolation<'a>>,
    exceptions: Vec<JsonException<'a>>,
    stale_exceptions: Vec<JsonException<'a>>,
    context_stale_exceptions: Vec<JsonException<'a>>,
    /// Entries naming a crate that is not a workspace member. Held apart from
    /// `stale_exceptions`, which means the narrower and older thing: the debt
    /// was real and has been paid off.
    unknown_crate_exceptions: Vec<JsonException<'a>>,
    context_unknown_crate_exceptions: Vec<JsonException<'a>>,
}

#[derive(Serialize)]
struct JsonPackage<'a> {
    name: &'a str,
    role: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<&'a str>,
}

#[derive(Serialize)]
struct JsonMatrixRow<'a> {
    consumer_role: &'a str,
    allowed_deps: Vec<&'a str>,
}

#[derive(Serialize)]
struct JsonRule<'a> {
    id: &'a str,
    description: &'a str,
    status: &'a str,
    failure_count: usize,
}

#[derive(Serialize)]
struct JsonViolation<'a> {
    axis: Axis,
    consumer: &'a str,
    consumer_role: &'a str,
    dep: &'a str,
    dep_role: &'a str,
    sanctioned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    ticket: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
    remediation: JsonRemediation<'a>,
}

#[derive(Serialize)]
struct JsonContextViolation<'a> {
    axis: Axis,
    consumer: &'a str,
    consumer_context: &'a str,
    dep: &'a str,
    dep_context: &'a str,
    sanctioned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    ticket: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
    remediation: JsonRemediation<'a>,
}

#[derive(Serialize)]
struct JsonRemediation<'a> {
    /// The complete rule as one sentence. For a role that is
    /// [`Role::contract`] — the generated allowed set plus the rationale — so
    /// a machine consumer reads the same constraint the text output shows,
    /// never a hand-written paraphrase of it. Owned because the allowed set is
    /// rendered, not stored.
    rule: String,
    fixes: &'a [&'a str],
}

#[derive(Serialize)]
struct JsonException<'a> {
    consumer: &'a str,
    dep: &'a str,
    axis: Axis,
    ticket: &'a str,
    reason: &'a str,
}

impl<'a> JsonException<'a> {
    fn from_lint(e: &'a Exception) -> Self {
        Self {
            consumer: &e.consumer,
            dep: &e.dep,
            axis: e.axis,
            ticket: &e.ticket,
            reason: &e.reason,
        }
    }
}
