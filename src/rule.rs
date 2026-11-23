//! Domain entity: the rules this lint enforces.
//!
//! Pure: no framework, no IO, no allocation beyond `&'static str`.
//!
//! ONE enumeration of the enforced rules, read by both the run report
//! (`--format json`) and the contract document (`--capabilities`). The two used
//! to be separate hand-written lists, which is the drift this file exists to
//! foreclose: a rule renamed in the report and not in the contract would tell an
//! agent it had been checked against something the tool does not check — a
//! contract that lies is worse than no contract.

use crate::lint::Axis;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Rule {
    /// Every workspace-internal edge respects the role matrix.
    RoleMatrix,
    /// No exceptions-file entry outlives the violation it sanctions, and none
    /// names a crate the workspace does not have.
    ExceptionsHonest,
    /// Every workspace-internal edge respects bounded-context isolation.
    ContextIsolation,
}

impl Rule {
    /// Every rule. The single enumeration: the JSON report's `rules` array, the
    /// capabilities document, and the tests all read this, so a rule added to
    /// the enum cannot be silently missing from any of them.
    pub const ALL: &'static [Self] = &[
        Self::RoleMatrix,
        Self::ExceptionsHonest,
        Self::ContextIsolation,
    ];

    /// Stable identifier. Part of the tool's published contract — an agent may
    /// key on it, so treat a change here as a breaking change.
    pub fn id(self) -> &'static str {
        match self {
            Self::RoleMatrix => "role-matrix",
            Self::ExceptionsHonest => "exceptions-honest",
            Self::ContextIsolation => "context-isolation",
        }
    }

    /// Which axis the rule belongs to. `ExceptionsHonest` spans both axes: the
    /// exceptions file is shared, and a stale entry is stale whichever axis it
    /// was written against.
    pub fn axis(self) -> Option<Axis> {
        match self {
            Self::RoleMatrix => Some(Axis::Role),
            Self::ContextIsolation => Some(Axis::Context),
            Self::ExceptionsHonest => None,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::RoleMatrix => {
                "Every workspace-internal dep edge respects the hex-arch role matrix (or is sanctioned by hex-lint-exceptions.toml)."
            }
            Self::ExceptionsHonest => {
                "Every entry in hex-lint-exceptions.toml names two workspace crates and corresponds to a real, current violation on its own axis (no stale debt, no crate that isn't there)."
            }
            Self::ContextIsolation => {
                "Every workspace-internal dep edge respects bounded-context isolation (same context, or the dependency is shared, or the consumer is a `composition-root` or a `tool`, each of which sits outside every context — a root to wire them together, a tool to read them all; or is sanctioned by hex-lint-exceptions.toml)."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Rule;
    use crate::lint::{Axis, OutsideEveryContext};
    use crate::role::Role;
    use std::collections::BTreeSet;

    #[test]
    fn all_lists_every_variant() {
        // Adding a variant makes this match non-exhaustive and the test stops
        // compiling, so extending `Rule::ALL` is enforced rather than hoped for.
        const fn tag(r: Rule) -> usize {
            match r {
                Rule::RoleMatrix => 0,
                Rule::ExceptionsHonest => 1,
                Rule::ContextIsolation => 2,
            }
        }
        assert_eq!(Rule::ALL.len(), tag(Rule::ContextIsolation) + 1);
    }

    #[test]
    fn ids_are_unique() {
        let ids: BTreeSet<&'static str> = Rule::ALL.iter().map(|&r: &Rule| r.id()).collect();
        assert_eq!(ids.len(), Rule::ALL.len(), "duplicate rule id");
    }

    #[test]
    fn every_rule_has_a_nonempty_id_and_description() {
        for &r in Rule::ALL {
            assert!(!r.id().is_empty(), "{r:?} has an empty id");
            assert!(
                !r.description().is_empty(),
                "{r:?} has an empty description"
            );
        }
    }

    #[test]
    fn each_axis_has_exactly_one_edge_rule() {
        // The two edge checks are per-axis; the exceptions rule is axis-neutral.
        // If a fourth rule ever lands on an existing axis, this fails and forces
        // the capabilities document's axis grouping to be reconsidered.
        for axis in [Axis::Role, Axis::Context] {
            let count: usize = Rule::ALL
                .iter()
                .filter(|&&r: &&Rule| r.axis() == Some(axis))
                .count();
            assert_eq!(count, 1, "expected one edge rule for {axis:?}");
        }
    }

    #[test]
    fn exceptions_rule_is_axis_neutral() {
        assert_eq!(Rule::ExceptionsHonest.axis(), None);
    }

    #[test]
    fn the_context_rule_states_its_exemption() {
        // This description is the rule as every JSON report and the published
        // contract state it. A reader who is placing a composition root or a
        // tool reads it before hex-lint ever judges an edge, so it has to name
        // every role that is not judged.
        let unstated: Vec<&'static str> = Role::ALL
            .iter()
            .filter(|&&r: &&Role| OutsideEveryContext::of(r).is_some())
            .map(|&r: &Role| r.as_str())
            .filter(|name: &&str| {
                !Rule::ContextIsolation
                    .description()
                    .contains(&format!("`{name}`"))
            })
            .collect();
        assert_eq!(unstated, Vec::<&'static str>::new());
    }
}
