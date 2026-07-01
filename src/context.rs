//! Domain entity: bounded-context isolation.
//!
//! Pure: no framework, no IO. This is the context axis and only the context
//! axis — it owns the reserved `shared` name, the total edge predicate, the
//! workspace-level adoption tri-state, and context remediation guidance. It
//! deliberately does NOT depend on `crate::role`: the two axes are orthogonal,
//! so a reader can delete this file (and its use case) without touching the
//! role axis, and vice versa. The only shared foundation is the neutral
//! `WorkspacePackage` DTO and the axis-neutral `Remediation`.

use crate::lint::WorkspacePackage;
use crate::remediation::Remediation;

/// The single reserved context name. Any crate may depend on a `shared` crate;
/// a `shared` crate may depend only on other `shared` crates.
pub const SHARED: &str = "shared";

/// The total context predicate over one dependency edge: a consumer in
/// `consumer_ctx` may depend on a crate in `dep_ctx` iff they share a context
/// or the dependency lives in the shared context. This already subsumes the
/// "shared may depend only on shared" rule: a `shared` consumer passes only
/// when `dep_ctx` is also `shared`.
pub fn allows(consumer_ctx: &str, dep_ctx: &str) -> bool {
    consumer_ctx == dep_ctx || dep_ctx == SHARED
}

/// Whether the workspace has opted into the context axis, and how completely.
/// Context isolation is all-or-nothing: either no member declares a context
/// (the axis is off), every member declares one (the axis is on), or the
/// declaration is half-finished — a hard error the tool refuses to run past.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Adoption {
    /// Zero members declare a context — the axis is off; behaviour is
    /// byte-identical to a role-only run.
    Disabled,
    /// Every member declares a context — the axis is on.
    Enabled,
    /// Some members declare a context and some do not, naming the members that
    /// are missing one. This is a hard error: partial adoption is meaningless.
    Partial(Vec<String>),
}

/// Resolve the workspace's context-axis adoption from its members. Reads each
/// member's `context`: `Disabled` when none declare it, `Enabled` when all do,
/// otherwise `Partial` listing — in workspace order — the members that lack a
/// context.
pub fn adoption(packages: &[WorkspacePackage]) -> Adoption {
    let declared: usize = packages
        .iter()
        .filter(|p: &&WorkspacePackage| p.context.is_some())
        .count();

    if declared == 0 {
        Adoption::Disabled
    } else if declared == packages.len() {
        Adoption::Enabled
    } else {
        let missing: Vec<String> = packages
            .iter()
            .filter(|p: &&WorkspacePackage| p.context.is_none())
            .map(|p: &WorkspacePackage| p.name.clone())
            .collect();
        Adoption::Partial(missing)
    }
}

/// Recovery guidance for a forbidden cross-context dependency. Pure static
/// advice, mirroring the role axis's remediation so both checks speak the same
/// "here is how to fix the architecture" language.
pub fn remediation() -> Remediation {
    Remediation {
        rule: "a crate may depend only on crates in its own context, or in the shared context; the shared context may depend only on shared.",
        fixes: &[
            "If the crate you're reaching into really belongs to your context, move it there — or extract just the part you need into a crate in your own context.",
            "If it's a genuine cross-context contract, promote the shared type or trait into the `shared` context and depend on that instead.",
            "If either crate's context tag is wrong, correct `package.metadata.hex-arch.context` to the context it actually belongs to.",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::{adoption, allows, remediation, Adoption, SHARED};
    // The role type is named only to fill `WorkspacePackage`'s mandatory `role`
    // field when building fixtures; no context logic here consults it.
    use crate::lint::WorkspacePackage;
    use crate::remediation::Remediation;
    use crate::role::Role;

    fn pkg(name: &str, context: Option<&str>) -> WorkspacePackage {
        WorkspacePackage {
            name: name.to_owned(),
            role: Role::Domain,
            context: context.map(|c: &str| c.to_owned()),
        }
    }

    #[test]
    fn same_context_is_allowed() {
        assert!(allows("shopping", "shopping"));
    }

    #[test]
    fn dependency_in_shared_is_allowed() {
        assert!(allows("shopping", SHARED));
    }

    #[test]
    fn cross_context_is_rejected() {
        assert!(!allows("shopping", "pantry"));
    }

    #[test]
    fn shared_consumer_reaching_non_shared_is_rejected() {
        assert!(!allows(SHARED, "pantry"));
    }

    #[test]
    fn zero_declared_is_disabled() {
        let packages: Vec<WorkspacePackage> = vec![pkg("a", None), pkg("b", None)];
        assert_eq!(adoption(&packages), Adoption::Disabled);
    }

    #[test]
    fn all_declared_is_enabled() {
        let packages: Vec<WorkspacePackage> =
            vec![pkg("a", Some("shopping")), pkg("b", Some("pantry"))];
        assert_eq!(adoption(&packages), Adoption::Enabled);
    }

    #[test]
    fn some_missing_is_partial_naming_exactly_the_missing_ones() {
        let packages: Vec<WorkspacePackage> = vec![
            pkg("a", Some("shopping")),
            pkg("b", None),
            pkg("c", Some("pantry")),
            pkg("d", None),
        ];
        assert_eq!(
            adoption(&packages),
            Adoption::Partial(vec!["b".to_owned(), "d".to_owned()])
        );
    }

    #[test]
    fn single_declared_member_is_enabled() {
        let packages: Vec<WorkspacePackage> = vec![pkg("solo", Some("shopping"))];
        assert_eq!(adoption(&packages), Adoption::Enabled);
    }

    #[test]
    fn remediation_states_the_rule_and_offers_a_concrete_fix() {
        let rem: Remediation = remediation();
        assert!(!rem.rule.is_empty());
        assert!(!rem.fixes.is_empty());
        for fix in rem.fixes {
            assert!(!fix.is_empty());
        }
    }
}
