//! Use case: compute the role-axis report from a workspace snapshot and a list
//! of sanctioned exceptions.
//!
//! Pure: no IO, no framework. The role matrix (`role.rs` domain entity) decides
//! which edges are legal; `lint::reconcile` does the axis-agnostic exception
//! bookkeeping. This file is the role axis and only the role axis — deleting it
//! must not touch the context axis.

use std::collections::BTreeMap;

use crate::lint::{reconcile, Axis, AxisReport, DepEdge, Exception, WorkspacePackage};
use crate::role::Role;

/// One role-matrix violation with both roles attached for reporting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleViolation {
    pub consumer: String,
    pub consumer_role: Role,
    pub dep: String,
    pub dep_role: Role,
}

/// Run the role check. `packages` MUST be the full set of workspace members;
/// any edge whose consumer or dep isn't in `packages` is dropped (foreign edge).
pub fn run(
    packages: &[WorkspacePackage],
    edges: &[DepEdge],
    exceptions: &[Exception],
) -> AxisReport<RoleViolation> {
    let role_by_name: BTreeMap<&str, Role> = packages
        .iter()
        .map(|p: &WorkspacePackage| (p.name.as_str(), p.role))
        .collect();

    let mut violations: Vec<RoleViolation> = Vec::new();
    for edge in edges {
        let (Some(&cr), Some(&dr)) = (
            role_by_name.get(edge.consumer.as_str()),
            role_by_name.get(edge.dep.as_str()),
        ) else {
            continue;
        };
        if !cr.allowed_deps().contains(&dr) {
            violations.push(RoleViolation {
                consumer: edge.consumer.clone(),
                consumer_role: cr,
                dep: edge.dep.clone(),
                dep_role: dr,
            });
        }
    }

    let role_exceptions: Vec<&Exception> = exceptions
        .iter()
        .filter(|e: &&Exception| e.axis == Axis::Role)
        .collect();

    reconcile(violations, &role_exceptions, |v: &RoleViolation| {
        (v.consumer.clone(), v.dep.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::{run, RoleViolation};
    use crate::lint::{Axis, AxisReport, DepEdge, Exception, WorkspacePackage};
    use crate::role::Role;

    fn pkg(name: &str, role: Role) -> WorkspacePackage {
        WorkspacePackage {
            name: name.to_owned(),
            role,
            context: None,
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
            axis: Axis::Role,
            ticket: "TICK-1".to_owned(),
            reason: "legacy".to_owned(),
        }
    }

    #[test]
    fn clean_workspace_reports_nothing() {
        let packages: Vec<WorkspacePackage> = vec![pkg("d", Role::Domain), pkg("u", Role::Usecase)];
        let edges: Vec<DepEdge> = vec![edge("u", "d")];
        let report: AxisReport<RoleViolation> = run(&packages, &edges, &[]);
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
        let report: AxisReport<RoleViolation> = run(&packages, &edges, &[]);
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.unsanctioned.len(), 1);
        assert_eq!(report.stale_exceptions.len(), 0);
        let v: &RoleViolation = &report.unsanctioned[0];
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
        let report: AxisReport<RoleViolation> = run(&packages, &edges, &exceptions);
        assert_eq!(report.violations.len(), 1);
        assert!(report.unsanctioned.is_empty());
        assert!(report.stale_exceptions.is_empty());
    }

    #[test]
    fn stale_exception_reported() {
        let packages: Vec<WorkspacePackage> = vec![pkg("d", Role::Domain), pkg("u", Role::Usecase)];
        let edges: Vec<DepEdge> = vec![edge("u", "d")]; // legal edge
        let exceptions: Vec<Exception> = vec![exc("u", "d")]; // sanctions a non-violation
        let report: AxisReport<RoleViolation> = run(&packages, &edges, &exceptions);
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
        let report: AxisReport<RoleViolation> = run(&packages, &edges, &[]);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn edges_with_unknown_endpoints_are_ignored() {
        let packages: Vec<WorkspacePackage> = vec![pkg("d", Role::Domain)];
        let edges: Vec<DepEdge> = vec![edge("d", "external_crate"), edge("external_crate", "d")];
        let report: AxisReport<RoleViolation> = run(&packages, &edges, &[]);
        assert!(report.violations.is_empty());
    }
}
