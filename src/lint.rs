//! Use-case core: the axis-agnostic plumbing both architecture checks stand on.
//!
//! Pure: no IO, no framework. Holds the shared DTOs built by the adapters
//! (`workspace.rs`, `exceptions.rs`), the `Axis` tag that routes an exception
//! to a check, a generic per-axis report, the generic exception reconciliation
//! (unsanctioned + stale + unknown-crate set arithmetic), and the one standing
//! both axes read — [`OutsideEveryContext`]. It enforces neither axis's rules: each axis
//! lives in its own use-case file and stands on this shared plumbing, so
//! neither ever has to name the other.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

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

/// Witness that a crate stands outside every bounded context instead of living
/// inside one, with the reason its declared role gives.
///
/// Each variant carries an [`Earned`], whose field is private, and
/// [`OutsideEveryContext::of`] is the only constructor, so a call site cannot
/// assert this standing: it can only be handed one that a role tag earned. Every
/// rule that exempts such a crate takes this value rather than re-reading the
/// tag, so no two of them can disagree about which crates stand outside, and a
/// surface that speaks for one cannot be reached without it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutsideEveryContext {
    /// The declared role is `composition-root`: it wires the contexts together.
    WiringRoot(Earned),
}

/// Proof that a role tag, not a call site, produced the enclosing witness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Earned(());

impl OutsideEveryContext {
    /// The witness `role` earns, if it earns one.
    pub fn of(role: Role) -> Option<Self> {
        match role {
            Role::CompositionRoot => Some(Self::WiringRoot(Earned(()))),
            Role::Kernel
            | Role::Domain
            | Role::Usecase
            | Role::PortAndAdapter
            | Role::DrivenAdapter
            | Role::DrivingAdapter
            | Role::Infra
            | Role::Tool => None,
        }
    }
}

impl WorkspacePackage {
    /// This package's witness of standing outside every context, if its role
    /// earns one.
    pub fn outside_every_context(&self) -> Option<OutsideEveryContext> {
        OutsideEveryContext::of(self.role)
    }
}

/// A workspace-internal dependency edge (runtime or build, not dev).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepEdge {
    pub consumer: String,
    pub dep: String,
}

/// Which check an exception sanctions. Defaults to `Role` so exception files
/// written before the context axis existed keep parsing unchanged.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
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
    /// This-axis exceptions that match no violation, but whose two crates are
    /// both workspace members — the debt was paid off and the entry outlived
    /// it. Delete it.
    pub stale_exceptions: Vec<Exception>,
    /// This-axis exceptions naming a crate that is not a workspace member at
    /// all. Also unmatched, but for a different reason and with a different
    /// fix: the name is a typo, or the crate was renamed or removed. Reporting
    /// these as "debt paid off" told the reader to celebrate a typo.
    pub unknown_crate_exceptions: Vec<Exception>,
}

/// The endpoints of `e` that are not workspace members.
///
/// Empty exactly when [`reconcile`] would call an unmatched `e` stale rather
/// than unknown-crate. One definition, so the classification and the message
/// explaining it can never disagree about which crate is missing.
pub fn unknown_endpoints<'a>(e: &'a Exception, known_packages: &BTreeSet<&str>) -> Vec<&'a str> {
    [e.consumer.as_str(), e.dep.as_str()]
        .into_iter()
        .filter(|name: &&str| !known_packages.contains(name))
        .collect()
}

/// Axis-agnostic exception bookkeeping. Given this axis's raw violations and
/// the exceptions already filtered to this axis, split violations into
/// sanctioned/unsanctioned, then split the exceptions that match no violation
/// into stale (both crates exist — debt paid off) and unknown-crate (a name
/// the workspace does not have). `key` projects a violation to its
/// `(consumer, dep)` identity; this function never mentions roles or contexts.
pub fn reconcile<V: Clone>(
    violations: Vec<V>,
    axis_exceptions: &[&Exception],
    known_packages: &BTreeSet<&str>,
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

    // An unmatched entry is dishonest either way, and both ways fail the lint.
    // They are split because the fix differs: a stale entry means delete it,
    // an unknown-crate entry means correct the name.
    let (stale_exceptions, unknown_crate_exceptions): (Vec<Exception>, Vec<Exception>) =
        axis_exceptions
            .iter()
            .copied()
            .filter(|e: &&Exception| !viol_keys.contains(&(e.consumer.clone(), e.dep.clone())))
            .cloned()
            .partition(|e: &Exception| unknown_endpoints(e, known_packages).is_empty());

    AxisReport {
        violations,
        unsanctioned,
        stale_exceptions,
        unknown_crate_exceptions,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        reconcile, unknown_endpoints, Axis, AxisReport, Earned, Exception, OutsideEveryContext,
    };
    use crate::role::Role;
    use std::collections::BTreeSet;

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
    fn the_composition_root_earns_the_wiring_witness() {
        assert_eq!(
            OutsideEveryContext::of(Role::CompositionRoot),
            Some(OutsideEveryContext::WiringRoot(Earned(())))
        );
    }

    #[test]
    fn exactly_the_composition_root_stands_outside_every_context() {
        // Quantified over the whole role enumeration and pinned as a literal,
        // so a role added later cannot quietly acquire the standing.
        let earned: Vec<&'static str> = Role::ALL
            .iter()
            .filter(|&&r: &&Role| OutsideEveryContext::of(r).is_some())
            .map(|&r: &Role| r.as_str())
            .collect();
        assert_eq!(earned, vec!["composition-root"]);
    }

    #[test]
    fn no_role_inside_the_contexts_may_depend_on_a_role_outside_them() {
        let bridges: Vec<(&'static str, &'static str)> = Role::ALL
            .iter()
            .filter(|&&consumer: &&Role| OutsideEveryContext::of(consumer).is_none())
            .flat_map(|&consumer: &Role| {
                consumer
                    .allowed_deps()
                    .iter()
                    .filter(|&&dep: &&Role| OutsideEveryContext::of(dep).is_some())
                    .map(move |&dep: &Role| (consumer.as_str(), dep.as_str()))
            })
            .collect();
        assert_eq!(bridges, Vec::<(&'static str, &'static str)>::new());
    }

    #[test]
    fn a_package_carries_the_witness_of_its_declared_role() {
        let root: super::WorkspacePackage = super::WorkspacePackage {
            name: "wiring".to_owned(),
            role: Role::CompositionRoot,
            context: None,
        };
        let leaf: super::WorkspacePackage = super::WorkspacePackage {
            name: "shell".to_owned(),
            role: Role::Usecase,
            context: None,
        };
        assert_eq!(
            root.outside_every_context(),
            OutsideEveryContext::of(Role::CompositionRoot)
        );
        assert_eq!(leaf.outside_every_context(), None);
    }

    /// The workspace every test below reconciles against: crates `a` … `d`.
    /// A name outside this set is a crate the workspace does not have.
    fn workspace() -> BTreeSet<&'static str> {
        BTreeSet::from(["a", "b", "c", "d"])
    }

    #[test]
    fn no_violations_and_no_exceptions_is_empty() {
        let report: AxisReport<(String, String)> = reconcile(Vec::new(), &[], &workspace(), key);
        assert!(report.violations.is_empty());
        assert!(report.unsanctioned.is_empty());
        assert!(report.stale_exceptions.is_empty());
    }

    #[test]
    fn violation_without_exception_is_unsanctioned() {
        let violations: Vec<(String, String)> = vec![("a".to_owned(), "b".to_owned())];
        let report: AxisReport<(String, String)> = reconcile(violations, &[], &workspace(), key);
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.unsanctioned.len(), 1);
        assert!(report.stale_exceptions.is_empty());
    }

    #[test]
    fn matching_exception_sanctions_and_leaves_no_stale() {
        let violations: Vec<(String, String)> = vec![("a".to_owned(), "b".to_owned())];
        let e: Exception = exc("a", "b", Axis::Role);
        let exceptions: Vec<&Exception> = vec![&e];
        let report: AxisReport<(String, String)> =
            reconcile(violations, &exceptions, &workspace(), key);
        assert_eq!(report.violations.len(), 1);
        assert!(report.unsanctioned.is_empty());
        assert!(report.stale_exceptions.is_empty());
    }

    #[test]
    fn exception_matching_no_violation_is_stale() {
        let e: Exception = exc("a", "b", Axis::Role);
        let exceptions: Vec<&Exception> = vec![&e];
        let report: AxisReport<(String, String)> =
            reconcile(Vec::new(), &exceptions, &workspace(), key);
        assert!(report.violations.is_empty());
        assert!(report.unsanctioned.is_empty());
        assert_eq!(report.stale_exceptions.len(), 1);
        assert_eq!(report.stale_exceptions[0].ticket, "TICK-1");
    }

    #[test]
    fn exception_naming_a_crate_outside_the_workspace_is_unknown_not_stale() {
        // "ghost" is not a workspace member. Calling this stale told the
        // reader the debt had been paid off, which is a congratulation for a
        // typo — and it hid that the real edge may still be there under the
        // right name.
        let e: Exception = exc("ghost", "b", Axis::Role);
        let exceptions: Vec<&Exception> = vec![&e];
        let report: AxisReport<(String, String)> =
            reconcile(Vec::new(), &exceptions, &workspace(), key);
        assert!(report.stale_exceptions.is_empty());
        assert_eq!(report.unknown_crate_exceptions.len(), 1);
        assert_eq!(report.unknown_crate_exceptions[0].ticket, "TICK-1");
    }

    #[test]
    fn a_matched_exception_is_neither_stale_nor_unknown() {
        // The zero case for both buckets: the entry is doing its job.
        let violations: Vec<(String, String)> = vec![("a".to_owned(), "b".to_owned())];
        let e: Exception = exc("a", "b", Axis::Role);
        let exceptions: Vec<&Exception> = vec![&e];
        let report: AxisReport<(String, String)> =
            reconcile(violations, &exceptions, &workspace(), key);
        assert!(report.stale_exceptions.is_empty());
        assert!(report.unknown_crate_exceptions.is_empty());
    }

    #[test]
    fn stale_and_unknown_are_reported_side_by_side() {
        // Many, and mixed: one entry of each kind must land in its own bucket
        // rather than one kind swallowing the other.
        let stale: Exception = exc("a", "b", Axis::Role);
        let unknown: Exception = exc("ghost", "phantom", Axis::Role);
        let exceptions: Vec<&Exception> = vec![&stale, &unknown];
        let report: AxisReport<(String, String)> =
            reconcile(Vec::new(), &exceptions, &workspace(), key);
        assert_eq!(report.stale_exceptions.len(), 1);
        assert_eq!(report.stale_exceptions[0].consumer, "a");
        assert_eq!(report.unknown_crate_exceptions.len(), 1);
        assert_eq!(report.unknown_crate_exceptions[0].consumer, "ghost");
    }

    #[test]
    fn unknown_endpoints_is_empty_when_both_crates_exist() {
        assert!(unknown_endpoints(&exc("a", "b", Axis::Role), &workspace()).is_empty());
    }

    #[test]
    fn unknown_endpoints_names_only_the_missing_side() {
        assert_eq!(
            unknown_endpoints(&exc("a", "phantom", Axis::Role), &workspace()),
            vec!["phantom"]
        );
    }

    #[test]
    fn unknown_endpoints_names_both_when_neither_crate_exists() {
        assert_eq!(
            unknown_endpoints(&exc("ghost", "phantom", Axis::Role), &workspace()),
            vec!["ghost", "phantom"]
        );
    }

    #[test]
    fn many_violations_split_by_exception_coverage() {
        let violations: Vec<(String, String)> = vec![
            ("a".to_owned(), "b".to_owned()),
            ("c".to_owned(), "d".to_owned()),
        ];
        let e: Exception = exc("a", "b", Axis::Role);
        let exceptions: Vec<&Exception> = vec![&e];
        let report: AxisReport<(String, String)> =
            reconcile(violations, &exceptions, &workspace(), key);
        assert_eq!(report.violations.len(), 2);
        assert_eq!(report.unsanctioned.len(), 1);
        assert_eq!(report.unsanctioned[0], ("c".to_owned(), "d".to_owned()));
        assert!(report.stale_exceptions.is_empty());
    }
}
