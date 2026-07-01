//! Use case: compute the context-axis report from a workspace snapshot and a
//! list of sanctioned exceptions.
//!
//! Pure: no IO, no framework. The context predicate (`context.rs` domain
//! entity) decides which edges are legal; `lint::reconcile` does the
//! axis-agnostic exception bookkeeping. This file is the context axis and only
//! the context axis — deleting it must not touch the role axis.

use std::collections::BTreeMap;

use crate::context;
use crate::lint::{reconcile, Axis, AxisReport, DepEdge, Exception, WorkspacePackage};

/// One context-isolation violation with both contexts attached for reporting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextViolation {
    pub consumer: String,
    pub consumer_context: String,
    pub dep: String,
    pub dep_context: String,
}

/// Run the context check. `packages` MUST be the full set of workspace members.
/// Any edge whose consumer or dep isn't in `packages` (foreign edge) or whose
/// endpoint declares no context is dropped — so with the axis disabled (every
/// context `None`) every edge is skipped and the report is naturally empty.
pub fn run(
    packages: &[WorkspacePackage],
    edges: &[DepEdge],
    exceptions: &[Exception],
) -> AxisReport<ContextViolation> {
    let context_by_name: BTreeMap<&str, Option<&str>> = packages
        .iter()
        .map(|p: &WorkspacePackage| (p.name.as_str(), p.context.as_deref()))
        .collect();

    let mut violations: Vec<ContextViolation> = Vec::new();
    for edge in edges {
        let (Some(&Some(cc)), Some(&Some(dc))) = (
            context_by_name.get(edge.consumer.as_str()),
            context_by_name.get(edge.dep.as_str()),
        ) else {
            continue;
        };
        if !context::allows(cc, dc) {
            violations.push(ContextViolation {
                consumer: edge.consumer.clone(),
                consumer_context: cc.to_owned(),
                dep: edge.dep.clone(),
                dep_context: dc.to_owned(),
            });
        }
    }

    let context_exceptions: Vec<&Exception> = exceptions
        .iter()
        .filter(|e: &&Exception| e.axis == Axis::Context)
        .collect();

    reconcile(violations, &context_exceptions, |v: &ContextViolation| {
        (v.consumer.clone(), v.dep.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::{run, ContextViolation};
    use crate::lint::{Axis, AxisReport, DepEdge, Exception, WorkspacePackage};
    use crate::role::Role;

    fn pkg(name: &str, context: Option<&str>) -> WorkspacePackage {
        WorkspacePackage {
            name: name.to_owned(),
            role: Role::Domain,
            context: context.map(|c: &str| c.to_owned()),
        }
    }

    fn edge(consumer: &str, dep: &str) -> DepEdge {
        DepEdge {
            consumer: consumer.to_owned(),
            dep: dep.to_owned(),
        }
    }

    fn exc(consumer: &str, dep: &str, axis: Axis) -> Exception {
        Exception {
            consumer: consumer.to_owned(),
            dep: dep.to_owned(),
            axis,
            ticket: "TICK-1".to_owned(),
            reason: "legacy".to_owned(),
        }
    }

    #[test]
    fn same_context_edge_is_clean() {
        let packages: Vec<WorkspacePackage> =
            vec![pkg("a", Some("shopping")), pkg("b", Some("shopping"))];
        let edges: Vec<DepEdge> = vec![edge("a", "b")];
        let report: AxisReport<ContextViolation> = run(&packages, &edges, &[]);
        assert!(report.violations.is_empty());
        assert!(report.unsanctioned.is_empty());
        assert!(report.stale_exceptions.is_empty());
    }

    #[test]
    fn cross_context_edge_is_one_violation_carrying_both_contexts() {
        let packages: Vec<WorkspacePackage> =
            vec![pkg("a", Some("shopping")), pkg("b", Some("pantry"))];
        let edges: Vec<DepEdge> = vec![edge("a", "b")];
        let report: AxisReport<ContextViolation> = run(&packages, &edges, &[]);
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.unsanctioned.len(), 1);
        assert_eq!(report.stale_exceptions.len(), 0);
        let v: &ContextViolation = &report.unsanctioned[0];
        assert_eq!(v.consumer, "a");
        assert_eq!(v.consumer_context, "shopping");
        assert_eq!(v.dep, "b");
        assert_eq!(v.dep_context, "pantry");
    }

    #[test]
    fn dependency_in_shared_is_clean() {
        let packages: Vec<WorkspacePackage> =
            vec![pkg("a", Some("shopping")), pkg("contracts", Some("shared"))];
        let edges: Vec<DepEdge> = vec![edge("a", "contracts")];
        let report: AxisReport<ContextViolation> = run(&packages, &edges, &[]);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn shared_consumer_reaching_non_shared_is_one_violation() {
        let packages: Vec<WorkspacePackage> = vec![
            pkg("contracts", Some("shared")),
            pkg("core", Some("pantry")),
        ];
        let edges: Vec<DepEdge> = vec![edge("contracts", "core")];
        let report: AxisReport<ContextViolation> = run(&packages, &edges, &[]);
        assert_eq!(report.violations.len(), 1);
        let v: &ContextViolation = &report.unsanctioned[0];
        assert_eq!(v.consumer_context, "shared");
        assert_eq!(v.dep_context, "pantry");
    }

    #[test]
    fn edge_with_context_less_endpoint_is_skipped() {
        // `b` opts out of the context axis, so the edge is dropped even though
        // a naive comparison of "shopping" vs None would look cross-context.
        let packages: Vec<WorkspacePackage> = vec![pkg("a", Some("shopping")), pkg("b", None)];
        let edges: Vec<DepEdge> = vec![edge("a", "b")];
        let report: AxisReport<ContextViolation> = run(&packages, &edges, &[]);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn context_axis_exception_suppresses_the_violation() {
        let packages: Vec<WorkspacePackage> =
            vec![pkg("a", Some("shopping")), pkg("b", Some("pantry"))];
        let edges: Vec<DepEdge> = vec![edge("a", "b")];
        let exceptions: Vec<Exception> = vec![exc("a", "b", Axis::Context)];
        let report: AxisReport<ContextViolation> = run(&packages, &edges, &exceptions);
        assert_eq!(report.violations.len(), 1);
        assert!(report.unsanctioned.is_empty());
        assert!(report.stale_exceptions.is_empty());
    }

    #[test]
    fn role_axis_exception_does_not_suppress_a_context_violation() {
        // Same (consumer, dep), but the exception is routed to the role axis;
        // the context check must ignore it and still report the violation.
        let packages: Vec<WorkspacePackage> =
            vec![pkg("a", Some("shopping")), pkg("b", Some("pantry"))];
        let edges: Vec<DepEdge> = vec![edge("a", "b")];
        let exceptions: Vec<Exception> = vec![exc("a", "b", Axis::Role)];
        let report: AxisReport<ContextViolation> = run(&packages, &edges, &exceptions);
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.unsanctioned.len(), 1);
        assert!(report.stale_exceptions.is_empty());
    }

    #[test]
    fn context_exception_matching_no_edge_is_stale() {
        let packages: Vec<WorkspacePackage> =
            vec![pkg("a", Some("shopping")), pkg("b", Some("shopping"))];
        let edges: Vec<DepEdge> = vec![edge("a", "b")]; // legal same-context edge
        let exceptions: Vec<Exception> = vec![exc("a", "b", Axis::Context)]; // sanctions a non-violation
        let report: AxisReport<ContextViolation> = run(&packages, &edges, &exceptions);
        assert!(report.violations.is_empty());
        assert!(report.unsanctioned.is_empty());
        assert_eq!(report.stale_exceptions.len(), 1);
        assert_eq!(report.stale_exceptions[0].ticket, "TICK-1");
    }
}
