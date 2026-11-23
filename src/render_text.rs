use std::collections::BTreeSet;

use crate::context;
use crate::context_check;
use crate::lint::{self, Axis, Exception};
use crate::remediation::Remediation;
use crate::role_check;

/// Report exceptions naming a crate this workspace does not have.
///
/// Kept apart from the stale-exception report because the two mean different
/// things and want different fixes: a stale entry is debt that was paid off
/// and should be deleted, while this one is a name that never resolved — the
/// debt it describes may well still be there, under the right name. Both axes
/// print the same shape, so both call this.
fn render_unknown_crate_exceptions(
    axis_label: &str,
    entries: &[Exception],
    known: &BTreeSet<&str>,
) {
    if entries.is_empty() {
        return;
    }
    eprintln!(
        "hex-lint: {axis_label}exceptions file entries naming a crate that is not in this workspace:"
    );
    eprintln!("(typo, or the crate was renamed or removed — correct the name, don't just delete the entry)");
    for e in entries {
        eprintln!(
            "  {} -> {}  [ticket={} reason={}]",
            e.consumer, e.dep, e.ticket, e.reason
        );
        eprintln!(
            "    not a workspace member: {}",
            lint::unknown_endpoints(e, known).join(", ")
        );
    }
}

pub fn render_text(
    packages: &[lint::WorkspacePackage],
    role_report: &lint::AxisReport<role_check::RoleViolation>,
    context_report: &lint::AxisReport<context_check::ContextViolation>,
    exceptions: &[Exception],
    context_enabled: bool,
    had_problem: bool,
) {
    let known: BTreeSet<&str> = packages
        .iter()
        .map(|p: &lint::WorkspacePackage| p.name.as_str())
        .collect();

    if !role_report.unsanctioned.is_empty() {
        eprintln!("hex-lint: unsanctioned hex-arch role violations (not in exceptions file):");
        for v in &role_report.unsanctioned {
            eprintln!(
                "  {} ({}) -> {} ({}): forbidden",
                v.consumer,
                v.consumer_role.as_str(),
                v.dep,
                v.dep_role.as_str()
            );
            let rem: Remediation = v.consumer_role.remediation();
            eprintln!("    why: {}", rem.rationale);
            // The allowed set, generated from the matrix. Without it the reader
            // sees only what the layer must not do, which does not always name
            // the rule that fired — an adapter reaching sideways into another
            // adapter used to read as advice about infra.
            eprintln!(
                "    may depend on: {} — and nothing else.",
                v.consumer_role.allowed_clause()
            );
            for fix in rem.fixes {
                eprintln!("    fix: {fix}");
            }
        }
        eprintln!(
            "    (run `hex-lint explain {}` for the full role contract)",
            role_report.unsanctioned[0].consumer_role.as_str()
        );
    }

    if !role_report.stale_exceptions.is_empty() {
        eprintln!("hex-lint: exceptions file entries that no longer match a real violation:");
        eprintln!("(remove them — debt paid off?)");
        for e in &role_report.stale_exceptions {
            eprintln!(
                "  {} -> {}  [ticket={} reason={}]",
                e.consumer, e.dep, e.ticket, e.reason
            );
        }
    }

    render_unknown_crate_exceptions("", &role_report.unknown_crate_exceptions, &known);

    if !context_report.unsanctioned.is_empty() {
        eprintln!("hex-lint: unsanctioned context-isolation violations (not in exceptions file):");
        for v in &context_report.unsanctioned {
            eprintln!(
                "  {} [{}] -> {} [{}]: forbidden",
                v.consumer, v.consumer_context, v.dep, v.dep_context
            );
        }
        let rem: Remediation = context::remediation();
        eprintln!("    why: {}", rem.rationale);
        for fix in rem.fixes {
            eprintln!("    fix: {fix}");
        }
    }

    if !context_report.stale_exceptions.is_empty() {
        eprintln!(
            "hex-lint: context-axis exceptions file entries that no longer match a real violation:"
        );
        eprintln!("(remove them — debt paid off?)");
        for e in &context_report.stale_exceptions {
            eprintln!(
                "  {} -> {}  [ticket={} reason={}]",
                e.consumer, e.dep, e.ticket, e.reason
            );
        }
    }

    render_unknown_crate_exceptions(
        "context-axis ",
        &context_report.unknown_crate_exceptions,
        &known,
    );

    if !had_problem {
        println!(
            "hex-lint: clean ({} workspace packages, {} active violation(s) all sanctioned by {} exception(s))",
            packages.len(),
            role_report.violations.len(),
            exceptions.len()
        );
        if context_enabled {
            let context_exception_count: usize = exceptions
                .iter()
                .filter(|e: &&Exception| e.axis == Axis::Context)
                .count();
            println!(
                "hex-lint: context isolation clean ({} active violation(s) all sanctioned by {} exception(s))",
                context_report.violations.len(),
                context_exception_count
            );
        }
    }
}
