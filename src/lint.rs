//! Use-case core: the axis-agnostic plumbing both architecture checks stand on.
//!
//! Pure: no IO, no framework. Holds the shared DTOs built by the adapters
//! (`workspace.rs`, `exceptions.rs`), the `Axis` tag that routes an exception
//! to a check, a generic per-axis report, and the generic exception
//! reconciliation (unsanctioned + stale set arithmetic). It knows nothing
//! about roles or contexts — each axis lives in its own use-case file and
//! stands on this shared plumbing.

use std::collections::BTreeSet;

use serde::Deserialize;

use crate::role::Role;

/// A workspace package paired with its declared role and optional bounded
/// context. Built by the cargo-metadata adapter; consumed by the checks. The
/// `context` is `None` until a member opts into the context axis; the context
/// domain (`context.rs`) reads it to decide adoption and legality.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspacePackage {
    pub name: String,
    pub role: Role,
    pub context: Option<String>,
}

/// A workspace-internal dependency edge (runtime or build, not dev).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepEdge {
    pub consumer: String,
    pub dep: String,
}

/// Which check an exception sanctions. Defaults to `Role` so exception files
/// written before the context axis existed keep parsing unchanged.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    #[default]
    Role,
    Context,
}

/// A sanctioned violation: "yes, $consumer depends on $dep against the $axis
/// rule, here is the ticket and reason."
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Exception {
    pub consumer: String,
    pub dep: String,
    #[serde(default)]
    pub axis: Axis,
    pub ticket: String,
    pub reason: String,
}

/// Output of one axis check. Generic over the axis's violation type so both
/// checks share the reporting shape without sharing their violation vocab.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AxisReport<V> {
    /// All violations on this axis, sanctioned or not.
    pub violations: Vec<V>,
    /// Violations not covered by any exception on this axis.
    pub unsanctioned: Vec<V>,
    /// This-axis exceptions that no longer match a real violation — debt paid off.
    pub stale_exceptions: Vec<Exception>,
}

/// Axis-agnostic exception bookkeeping. Given this axis's raw violations and
/// the exceptions already filtered to this axis, split violations into
/// sanctioned/unsanctioned and surface exceptions that match no violation
/// (stale). `key` projects a violation to its `(consumer, dep)` identity; this
/// function never mentions roles or contexts.
pub fn reconcile<V: Clone>(
    violations: Vec<V>,
    axis_exceptions: &[&Exception],
    key: impl Fn(&V) -> (String, String),
) -> AxisReport<V> {
    let viol_keys: BTreeSet<(String, String)> = violations.iter().map(|v: &V| key(v)).collect();
    let exc_keys: BTreeSet<(String, String)> = axis_exceptions
        .iter()
        .map(|e: &&Exception| (e.consumer.clone(), e.dep.clone()))
        .collect();

    let unsanctioned: Vec<V> = violations
        .iter()
        .filter(|v: &&V| !exc_keys.contains(&key(v)))
        .cloned()
        .collect();

    let stale_exceptions: Vec<Exception> = axis_exceptions
        .iter()
        .copied()
        .filter(|e: &&Exception| !viol_keys.contains(&(e.consumer.clone(), e.dep.clone())))
        .cloned()
        .collect();

    AxisReport {
        violations,
        unsanctioned,
        stale_exceptions,
    }
}

#[cfg(test)]
mod tests {
    use super::{reconcile, Axis, AxisReport, Exception};

    fn exc(consumer: &str, dep: &str, axis: Axis) -> Exception {
        Exception {
            consumer: consumer.to_owned(),
            dep: dep.to_owned(),
            axis,
            ticket: "TICK-1".to_owned(),
            reason: "legacy".to_owned(),
        }
    }

    fn key(v: &(String, String)) -> (String, String) {
        v.clone()
    }

    #[test]
    fn no_violations_and_no_exceptions_is_empty() {
        let report: AxisReport<(String, String)> = reconcile(Vec::new(), &[], key);
        assert!(report.violations.is_empty());
        assert!(report.unsanctioned.is_empty());
        assert!(report.stale_exceptions.is_empty());
    }

    #[test]
    fn violation_without_exception_is_unsanctioned() {
        let violations: Vec<(String, String)> = vec![("a".to_owned(), "b".to_owned())];
        let report: AxisReport<(String, String)> = reconcile(violations, &[], key);
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.unsanctioned.len(), 1);
        assert!(report.stale_exceptions.is_empty());
    }

    #[test]
    fn matching_exception_sanctions_and_leaves_no_stale() {
        let violations: Vec<(String, String)> = vec![("a".to_owned(), "b".to_owned())];
        let e: Exception = exc("a", "b", Axis::Role);
        let exceptions: Vec<&Exception> = vec![&e];
        let report: AxisReport<(String, String)> = reconcile(violations, &exceptions, key);
        assert_eq!(report.violations.len(), 1);
        assert!(report.unsanctioned.is_empty());
        assert!(report.stale_exceptions.is_empty());
    }

    #[test]
    fn exception_matching_no_violation_is_stale() {
        let e: Exception = exc("a", "b", Axis::Role);
        let exceptions: Vec<&Exception> = vec![&e];
        let report: AxisReport<(String, String)> = reconcile(Vec::new(), &exceptions, key);
        assert!(report.violations.is_empty());
        assert!(report.unsanctioned.is_empty());
        assert_eq!(report.stale_exceptions.len(), 1);
        assert_eq!(report.stale_exceptions[0].ticket, "TICK-1");
    }

    #[test]
    fn many_violations_split_by_exception_coverage() {
        let violations: Vec<(String, String)> = vec![
            ("a".to_owned(), "b".to_owned()),
            ("c".to_owned(), "d".to_owned()),
        ];
        let e: Exception = exc("a", "b", Axis::Role);
        let exceptions: Vec<&Exception> = vec![&e];
        let report: AxisReport<(String, String)> = reconcile(violations, &exceptions, key);
        assert_eq!(report.violations.len(), 2);
        assert_eq!(report.unsanctioned.len(), 1);
        assert_eq!(report.unsanctioned[0], ("c".to_owned(), "d".to_owned()));
        assert!(report.stale_exceptions.is_empty());
    }
}
