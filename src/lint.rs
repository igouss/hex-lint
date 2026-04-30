//! Use case: compute hex-arch lint report from a workspace snapshot and a
//! list of sanctioned exceptions.
//!
//! Pure: no IO, no framework. The adapters in `workspace.rs` and
//! `exceptions.rs` build the inputs; this function decides the verdict.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::role::Role;

/// A workspace package paired with its declared role. Built by the
/// cargo-metadata adapter; consumed by the lint use case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspacePackage {
    pub name: String,
    pub role: Role,
}

/// A workspace-internal dependency edge (runtime or build, not dev).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepEdge {
    pub consumer: String,
    pub dep: String,
}

/// A sanctioned violation: "yes, $consumer depends on $dep against the
/// matrix, here is the ticket and reason."
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Exception {
    pub consumer: String,
    pub dep: String,
    pub ticket: String,
    pub reason: String,
}

/// One matrix violation with both roles attached for reporting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Violation {
    pub consumer: String,
    pub consumer_role: Role,
    pub dep: String,
    pub dep_role: Role,
}

/// Output of the lint use case.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LintReport {
    /// All matrix violations, sanctioned or not.
    pub violations: Vec<Violation>,
    /// Violations not covered by any exception.
    pub unsanctioned: Vec<Violation>,
    /// Exceptions that no longer match any real violation — debt paid off.
    pub stale_exceptions: Vec<Exception>,
}

/// Run the lint. `packages` MUST be the full set of workspace members; any
/// edge whose consumer or dep isn't in `packages` is dropped (foreign edge).
pub fn run(
    packages: &[WorkspacePackage],
    edges: &[DepEdge],
    exceptions: &[Exception],
) -> LintReport {
    let role_by_name: BTreeMap<&str, Role> =
        packages.iter().map(|p| (p.name.as_str(), p.role)).collect();

    let mut violations: Vec<Violation> = Vec::new();
    for edge in edges {
        let (Some(&cr), Some(&dr)) = (
            role_by_name.get(edge.consumer.as_str()),
            role_by_name.get(edge.dep.as_str()),
        ) else {
            continue;
        };
        if !cr.allowed_deps().contains(&dr) {
            violations.push(Violation {
                consumer: edge.consumer.clone(),
                consumer_role: cr,
                dep: edge.dep.clone(),
                dep_role: dr,
            });
        }
    }

    let viol_keys: BTreeSet<(&str, &str)> = violations
        .iter()
        .map(|v| (v.consumer.as_str(), v.dep.as_str()))
        .collect();
    let exc_keys: BTreeSet<(&str, &str)> = exceptions
        .iter()
        .map(|e| (e.consumer.as_str(), e.dep.as_str()))
        .collect();

    let unsanctioned: Vec<Violation> = violations
        .iter()
        .filter(|v| !exc_keys.contains(&(v.consumer.as_str(), v.dep.as_str())))
        .cloned()
        .collect();

    let stale_exceptions: Vec<Exception> = exceptions
        .iter()
        .filter(|e| !viol_keys.contains(&(e.consumer.as_str(), e.dep.as_str())))
        .cloned()
        .collect();

    LintReport {
        violations,
        unsanctioned,
        stale_exceptions,
    }
}

#[cfg(test)]
mod tests {
    use super::{run, DepEdge, Exception, LintReport, Violation, WorkspacePackage};
    use crate::role::Role;

    fn pkg(name: &str, role: Role) -> WorkspacePackage {
        WorkspacePackage {
            name: name.to_owned(),
            role,
        }
    }

    fn edge(consumer: &str, dep: &str) -> DepEdge {
        DepEdge {
            consumer: consumer.to_owned(),
            dep: dep.to_owned(),
        }
    }

    fn exc(consumer: &str, dep: &str) -> Exception {
        Exception {
            consumer: consumer.to_owned(),
            dep: dep.to_owned(),
            ticket: "TICK-1".to_owned(),
            reason: "legacy".to_owned(),
        }
    }

    #[test]
    fn clean_workspace_reports_nothing() {
        let packages: Vec<WorkspacePackage> = vec![pkg("d", Role::Domain), pkg("u", Role::Usecase)];
        let edges: Vec<DepEdge> = vec![edge("u", "d")];
        let report: LintReport = run(&packages, &edges, &[]);
        assert!(report.violations.is_empty());
        assert!(report.unsanctioned.is_empty());
        assert!(report.stale_exceptions.is_empty());
    }

    #[test]
    fn matrix_violation_is_unsanctioned_without_exception() {
        let packages: Vec<WorkspacePackage> =
            vec![pkg("d", Role::Domain), pkg("infra", Role::Infra)];
        // Domain depending on Infra is forbidden.
        let edges: Vec<DepEdge> = vec![edge("d", "infra")];
        let report: LintReport = run(&packages, &edges, &[]);
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.unsanctioned.len(), 1);
        assert_eq!(report.stale_exceptions.len(), 0);
        let v: &Violation = &report.unsanctioned[0];
        assert_eq!(v.consumer, "d");
        assert_eq!(v.consumer_role, Role::Domain);
        assert_eq!(v.dep, "infra");
        assert_eq!(v.dep_role, Role::Infra);
    }

    #[test]
    fn exception_sanctions_violation() {
        let packages: Vec<WorkspacePackage> =
            vec![pkg("d", Role::Domain), pkg("infra", Role::Infra)];
        let edges: Vec<DepEdge> = vec![edge("d", "infra")];
        let exceptions: Vec<Exception> = vec![exc("d", "infra")];
        let report: LintReport = run(&packages, &edges, &exceptions);
        assert_eq!(report.violations.len(), 1);
        assert!(report.unsanctioned.is_empty());
        assert!(report.stale_exceptions.is_empty());
    }

    #[test]
    fn stale_exception_reported() {
        let packages: Vec<WorkspacePackage> = vec![pkg("d", Role::Domain), pkg("u", Role::Usecase)];
        let edges: Vec<DepEdge> = vec![edge("u", "d")]; // legal edge
        let exceptions: Vec<Exception> = vec![exc("u", "d")]; // sanctions a non-violation
        let report: LintReport = run(&packages, &edges, &exceptions);
        assert!(report.violations.is_empty());
        assert!(report.unsanctioned.is_empty());
        assert_eq!(report.stale_exceptions.len(), 1);
        assert_eq!(report.stale_exceptions[0].ticket, "TICK-1");
    }

    #[test]
    fn legal_edges_ignored() {
        // composition-root may depend on anything.
        let packages: Vec<WorkspacePackage> = vec![
            pkg("root", Role::CompositionRoot),
            pkg("d", Role::Domain),
            pkg("u", Role::Usecase),
            pkg("infra", Role::Infra),
        ];
        let edges: Vec<DepEdge> = vec![edge("root", "d"), edge("root", "u"), edge("root", "infra")];
        let report: LintReport = run(&packages, &edges, &[]);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn edges_with_unknown_endpoints_are_ignored() {
        let packages: Vec<WorkspacePackage> = vec![pkg("d", Role::Domain)];
        let edges: Vec<DepEdge> = vec![edge("d", "external_crate"), edge("external_crate", "d")];
        let report: LintReport = run(&packages, &edges, &[]);
        assert!(report.violations.is_empty());
    }
}
