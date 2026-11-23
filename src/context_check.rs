//! Use case: compute the context-axis report from a workspace snapshot and a
//! list of sanctioned exceptions.
//!
//! Pure: no IO, no framework. The context predicate (`context.rs` domain
//! entity) decides which edges are legal; `lint::reconcile` does the
//! axis-agnostic exception bookkeeping. This file is the context axis and only
//! the context axis — deleting it must not touch the role axis.

use std::collections::{BTreeMap, BTreeSet};

use crate::context::{self, Standing};
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
/// An edge out of a crate that stands outside the contexts is not judged; the
/// consumer's standing comes from the witness its role earned, so this file
/// never reads a role tag of its own.
pub fn run(
    packages: &[WorkspacePackage],
    edges: &[DepEdge],
    exceptions: &[Exception],
) -> AxisReport<ContextViolation> {
    let by_name: BTreeMap<&str, &WorkspacePackage> = packages
        .iter()
        .map(|p: &WorkspacePackage| (p.name.as_str(), p))
        .collect();

    let mut violations: Vec<ContextViolation> = Vec::new();
    for edge in edges {
        let (Some(consumer), Some(dep)) = (
            by_name.get(edge.consumer.as_str()),
            by_name.get(edge.dep.as_str()),
        ) else {
            continue;
        };
        let (Some(cc), Some(dc)) = (consumer.context.as_deref(), dep.context.as_deref()) else {
            continue;
        };
        if !context::allows(Standing::of(cc, consumer.outside_every_context()), dc) {
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

    let known: BTreeSet<&str> = packages
        .iter()
        .map(|p: &WorkspacePackage| p.name.as_str())
        .collect();

    reconcile(
        violations,
        &context_exceptions,
        &known,
        |v: &ContextViolation| (v.consumer.clone(), v.dep.clone()),
    )
}

#[cfg(test)]
mod tests {
    use super::{run, ContextViolation};
    use crate::lint::{
        Axis, AxisReport, DepEdge, Exception, OutsideEveryContext, WorkspacePackage,
    };
    use crate::role::Role;

    fn pkg(name: &str, context: Option<&str>) -> WorkspacePackage {
        WorkspacePackage {
            name: name.to_owned(),
            role: Role::Domain,
            context: context.map(|c: &str| c.to_owned()),
        }
    }

    /// A package that wires the system together: the one role whose edges this
    /// axis does not judge.
    fn root(name: &str, context: Option<&str>) -> WorkspacePackage {
        WorkspacePackage {
            name: name.to_owned(),
            role: Role::CompositionRoot,
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

    // --- the composition-root exemption: zero, one and many edges out of a
    //     root, and the same edges out of a crate that is not one.

    #[test]
    fn a_root_that_leaves_its_own_context_alone_is_clean() {
        let packages: Vec<WorkspacePackage> =
            vec![root("wiring", Some("app")), pkg("helper", Some("app"))];
        let edges: Vec<DepEdge> = vec![edge("wiring", "helper")];
        let report: AxisReport<ContextViolation> = run(&packages, &edges, &[]);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn a_root_reaching_one_other_context_is_clean() {
        let packages: Vec<WorkspacePackage> = vec![
            root("wiring", Some("app")),
            pkg("shopping", Some("shopping")),
        ];
        let edges: Vec<DepEdge> = vec![edge("wiring", "shopping")];
        let report: AxisReport<ContextViolation> = run(&packages, &edges, &[]);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn a_root_reaching_two_other_contexts_is_clean() {
        let packages: Vec<WorkspacePackage> = vec![
            root("wiring", Some("app")),
            pkg("shopping", Some("shopping")),
            pkg("pantry", Some("pantry")),
        ];
        let edges: Vec<DepEdge> = vec![edge("wiring", "shopping"), edge("wiring", "pantry")];
        let report: AxisReport<ContextViolation> = run(&packages, &edges, &[]);
        assert!(report.violations.is_empty());
        assert!(report.unsanctioned.is_empty());
    }

    #[test]
    fn a_crate_that_is_not_a_root_reaching_the_same_two_contexts_is_two_violations() {
        // The contrast that makes the exemption an exemption rather than a
        // loosening: identical edges, identical contexts, no witness.
        let packages: Vec<WorkspacePackage> = vec![
            pkg("wiring", Some("app")),
            pkg("shopping", Some("shopping")),
            pkg("pantry", Some("pantry")),
        ];
        let edges: Vec<DepEdge> = vec![edge("wiring", "shopping"), edge("wiring", "pantry")];
        let report: AxisReport<ContextViolation> = run(&packages, &edges, &[]);
        assert_eq!(report.violations.len(), 2);
        assert_eq!(report.unsanctioned.len(), 2);
    }

    /// Every context pair this axis can tell apart. The predicate reads its two
    /// arguments only through `==` against each other and against the reserved
    /// name, so these four classes exhaust its domain up to renaming.
    const CONTEXT_CLASSES: &[(&str, &str)] = &[
        ("shopping", "shopping"),
        ("shopping", "shared"),
        ("shopping", "pantry"),
        ("shared", "pantry"),
    ];

    /// Whether a consumer in `cc` may depend on a crate in `dc`, as the axis
    /// read it before any exemption existed. Written out here rather than
    /// called from `context`, so the property below tests the rule instead of
    /// restating the implementation.
    fn isolated(cc: &str, dc: &str) -> bool {
        cc == dc || dc == "shared"
    }

    /// Every (role, consumer context, dependency context) the property covers.
    fn cases() -> impl Iterator<Item = (Role, &'static str, &'static str)> {
        Role::ALL.iter().flat_map(|&role: &Role| {
            CONTEXT_CLASSES
                .iter()
                .map(move |&(cc, dc): &(&'static str, &'static str)| (role, cc, dc))
        })
    }

    /// How many violations the axis reports for a lone edge out of a crate with
    /// `role` in context `cc`, into a crate in context `dc`.
    fn verdict(role: Role, cc: &str, dc: &str) -> usize {
        let packages: Vec<WorkspacePackage> = vec![
            WorkspacePackage {
                name: "consumer".to_owned(),
                role,
                context: Some(cc.to_owned()),
            },
            pkg("dep", Some(dc)),
        ];
        let edges: Vec<DepEdge> = vec![edge("consumer", "dep")];
        run(&packages, &edges, &[]).violations.len()
    }

    #[test]
    fn the_exemption_changes_the_verdict_only_for_a_wiring_root() {
        // Quantified over every role the tool knows, crossed with every context
        // class: the axis answers exactly what it answered before the exemption
        // existed, except out of a crate holding the wiring witness, where it
        // answers "no violation" whatever the contexts are.
        let actual: Vec<(&str, &str, &str, usize)> = cases()
            .map(|(role, cc, dc): (Role, &str, &str)| {
                (role.as_str(), cc, dc, verdict(role, cc, dc))
            })
            .collect();
        let expected: Vec<(&str, &str, &str, usize)> = cases()
            .map(|(role, cc, dc): (Role, &str, &str)| {
                let unexempt: usize = usize::from(!isolated(cc, dc));
                (
                    role.as_str(),
                    cc,
                    dc,
                    OutsideEveryContext::of(role).map_or(unexempt, |_: OutsideEveryContext| 0),
                )
            })
            .collect();
        assert_eq!(actual, expected);
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
