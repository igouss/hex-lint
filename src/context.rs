//! Domain entity: bounded-context isolation.
//!
//! Pure: no framework, no IO. This is the context axis and only the context
//! axis — it owns the reserved `shared` name, the total edge predicate, the
//! workspace-level adoption tri-state, and context remediation guidance. It
//! deliberately does NOT depend on `crate::role`: the two axes are orthogonal,
//! so a reader can delete this file (and its use case) without touching the
//! role axis, and vice versa. The only shared foundation is the neutral
//! `WorkspacePackage` DTO, the `WiringRoot` standing it carries, and the
//! axis-neutral `Remediation`.

use crate::lint::{WiringRoot, WorkspacePackage};
use crate::remediation::Remediation;

/// The single reserved context name. Any crate may depend on a `shared` crate;
/// a `shared` crate may depend only on other `shared` crates.
pub const SHARED: &str = "shared";

/// The context rule in one sentence, its exemption included. The single
/// statement of it: the remediation rationale, the published contract and
/// `explain` all read this, so no surface can state the rule and leave the
/// exemption out.
pub const RULE: &str = "a crate may depend only on crates in its own context, or in the shared context; the shared context may depend only on shared. A composition root is exempt — it sits outside every context in order to wire them together, and nothing may depend on a composition root, so the exemption opens no way into a context.";

/// Where the consumer of a dependency edge stands with respect to the contexts.
///
/// A crate either lives inside exactly one bounded context, and everything it
/// names must be in that context or in `shared`; or it is a wiring root, which
/// sits outside every context precisely so that it can join them, and whose
/// edges this axis does not judge. A sum rather than a context plus a flag, so
/// no caller can ask about an edge without first saying where its consumer
/// stands — and `Wiring` cannot be claimed, only presented: it carries the
/// witness a role tag earns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Standing<'a> {
    /// Inside the named bounded context.
    Inside(&'a str),
    /// Outside every bounded context, wiring them together.
    Wiring(WiringRoot),
}

impl<'a> Standing<'a> {
    /// Where a consumer that declares `context` stands, given the witness its
    /// role earned — if it earned one.
    pub fn of(context: &'a str, wiring: Option<WiringRoot>) -> Self {
        match wiring {
            Some(root) => Self::Wiring(root),
            None => Self::Inside(context),
        }
    }
}

/// The total context predicate over one dependency edge: a consumer inside
/// `consumer_ctx` may depend on a crate in `dep_ctx` iff they share a context
/// or the dependency lives in the shared context. This already subsumes the
/// "shared may depend only on shared" rule: a `shared` consumer passes only
/// when `dep_ctx` is also `shared`. A consumer that stands outside the contexts
/// is not judged at all — see [`RULE`].
pub fn allows(consumer: Standing<'_>, dep_ctx: &str) -> bool {
    match consumer {
        Standing::Wiring(_) => true,
        Standing::Inside(consumer_ctx) => consumer_ctx == dep_ctx || dep_ctx == SHARED,
    }
}

/// The rule as it reads to a crate that stands outside the contexts. Takes the
/// witness, so this axis cannot tell a crate it is exempt unless it is.
pub fn exemption_note(_: WiringRoot) -> &'static str {
    RULE
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
        // The context axis has no generated clause to defer to: its allowed set
        // is a predicate over two tags, not a matrix row, so `RULE` is the only
        // statement of it. The role axis's rule against enumerating does not
        // apply.
        rationale: RULE,
        fixes: &[
            "If the crate you're reaching into really belongs to your context, move it there — or extract just the part you need into a crate in your own context.",
            "If it's a genuine cross-context contract, promote the shared type or trait into the `shared` context and depend on that instead.",
            "If either crate's context tag is wrong, correct `package.metadata.hex-arch.context` to the context it actually belongs to.",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::{adoption, allows, exemption_note, remediation, Adoption, Standing, RULE, SHARED};
    // The role type is named only to fill `WorkspacePackage`'s mandatory `role`
    // field when building fixtures, and to mint the wiring witness this axis
    // consumes but never produces; no context logic here consults it.
    use crate::lint::{WiringRoot, WorkspacePackage};
    use crate::remediation::Remediation;
    use crate::role::Role;

    fn pkg(name: &str, context: Option<&str>) -> WorkspacePackage {
        WorkspacePackage {
            name: name.to_owned(),
            role: Role::Domain,
            context: context.map(|c: &str| c.to_owned()),
        }
    }

    /// The witness a `composition-root` tag earns.
    fn wiring() -> WiringRoot {
        WiringRoot::of(Role::CompositionRoot).expect("composition-root earns the wiring witness")
    }

    #[test]
    fn same_context_is_allowed() {
        assert!(allows(Standing::Inside("shopping"), "shopping"));
    }

    #[test]
    fn dependency_in_shared_is_allowed() {
        assert!(allows(Standing::Inside("shopping"), SHARED));
    }

    #[test]
    fn cross_context_is_rejected() {
        assert!(!allows(Standing::Inside("shopping"), "pantry"));
    }

    #[test]
    fn shared_consumer_reaching_non_shared_is_rejected() {
        assert!(!allows(Standing::Inside(SHARED), "pantry"));
    }

    #[test]
    fn a_wiring_root_reaching_another_context_is_allowed() {
        assert!(allows(Standing::Wiring(wiring()), "pantry"));
    }

    #[test]
    fn a_consumer_without_the_witness_stands_inside_its_context() {
        assert_eq!(Standing::of("shopping", None), Standing::Inside("shopping"));
    }

    #[test]
    fn a_consumer_with_the_witness_stands_outside_its_context() {
        assert_eq!(
            Standing::of("shopping", Some(wiring())),
            Standing::Wiring(wiring())
        );
    }

    #[test]
    fn the_stated_rule_carries_the_exemption() {
        assert!(
            RULE.contains("composition root is exempt"),
            "the rule must state its exemption: {RULE}"
        );
        assert_eq!(exemption_note(wiring()), RULE);
        assert_eq!(remediation().rationale, RULE);
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
        assert!(!rem.rationale.is_empty());
        assert!(!rem.fixes.is_empty());
        for fix in rem.fixes {
            assert!(!fix.is_empty());
        }
    }
}
