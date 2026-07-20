//! Domain entity: hexagonal-architecture role.
//!
//! Pure: no framework, no IO, no allocation beyond `&'static str`.

use crate::remediation::Remediation;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Role {
    Domain,
    Usecase,
    PortAndAdapter,
    DrivenAdapter,
    DrivingAdapter,
    Infra,
    CompositionRoot,
}

impl Role {
    /// Every role, in outward order. The single enumeration: the JSON matrix,
    /// `explain`'s role listing, and the matrix property tests all read this,
    /// so a role added to the enum cannot be silently missing from any of them.
    pub const ALL: &'static [Self] = &[
        Self::Domain,
        Self::Usecase,
        Self::PortAndAdapter,
        Self::DrivenAdapter,
        Self::DrivingAdapter,
        Self::Infra,
        Self::CompositionRoot,
    ];

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "domain" => Self::Domain,
            "usecase" => Self::Usecase,
            "port-and-adapter" => Self::PortAndAdapter,
            "driven-adapter" => Self::DrivenAdapter,
            "driving-adapter" => Self::DrivingAdapter,
            "infra" => Self::Infra,
            "composition-root" => Self::CompositionRoot,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Domain => "domain",
            Self::Usecase => "usecase",
            Self::PortAndAdapter => "port-and-adapter",
            Self::DrivenAdapter => "driven-adapter",
            Self::DrivingAdapter => "driving-adapter",
            Self::Infra => "infra",
            Self::CompositionRoot => "composition-root",
        }
    }

    /// Roles a consumer with `self` may legally depend on. Strict hex matrix.
    pub fn allowed_deps(self) -> &'static [Self] {
        use Role::{
            CompositionRoot, Domain, DrivenAdapter, DrivingAdapter, Infra, PortAndAdapter, Usecase,
        };
        match self {
            Domain => &[Domain],
            Usecase => &[Domain, Usecase, PortAndAdapter],
            PortAndAdapter => &[Domain, PortAndAdapter],
            DrivenAdapter => &[Domain, PortAndAdapter, Infra],
            DrivingAdapter => &[Domain, Usecase, PortAndAdapter],
            Infra => &[Infra],
            CompositionRoot => &[
                Domain,
                Usecase,
                PortAndAdapter,
                DrivenAdapter,
                DrivingAdapter,
                Infra,
                CompositionRoot,
            ],
        }
    }

    /// Recovery guidance for a forbidden dependency *out of* a crate with this
    /// role. Pure static advice; total over every role (composition-root can't
    /// actually produce a violation, but the function still answers).
    pub fn remediation(self) -> Remediation {
        match self {
            Self::Domain => Remediation {
                rule: "domain is the pure heart of the system: it may depend on nothing but other domain crates — no frameworks, no I/O, no adapters.",
                fixes: &[
                    "If this crate isn't really pure domain, it's mislabeled — give it the role it actually plays (usecase, infra, or an adapter).",
                    "If domain genuinely needs a behavior from outside, invert it: declare a port (trait) in a port-and-adapter crate and depend on the trait, never the implementation.",
                ],
            },
            Self::Usecase => Remediation {
                rule: "a usecase orchestrates application behavior and may reach the outside world only through ports — never an adapter or infra crate directly.",
                fixes: &[
                    "Declare the capability you need as a port (trait) in a port-and-adapter crate and depend on that; a driven-adapter implements it.",
                    "Let the composition-root inject the concrete implementation — the usecase only ever names the trait.",
                ],
            },
            Self::PortAndAdapter => Remediation {
                rule: "a port-and-adapter crate holds trait definitions and the domain types they speak in; it may depend only on domain and other port-and-adapter crates.",
                fixes: &[
                    "If you're reaching for a usecase or an adapter, this crate is mis-scoped — split the offending code out into the layer that owns it.",
                    "If the type you need is a plain data type, it belongs in domain — move it there and depend on domain.",
                ],
            },
            Self::DrivenAdapter => Remediation {
                rule: "a driven-adapter implements a port against real infrastructure; it may depend on domain, port-and-adapter, and infra — never up into usecases or sideways into other adapters.",
                fixes: &[
                    "If two adapters need to share code, extract it into an infra crate they both depend on.",
                    "If this is genuine wiring between collaborators, it belongs in the composition-root, not inside the adapter.",
                ],
            },
            Self::DrivingAdapter => Remediation {
                rule: "a driving-adapter turns external input into a usecase call; it may depend on domain, usecase, and port-and-adapter — never on driven adapters or infra directly.",
                fixes: &[
                    "Whatever infra or driven-adapter you're reaching for should be injected by the composition-root, already wired.",
                    "If you're tempted to instantiate an adapter here, that construction belongs in the composition-root.",
                ],
            },
            Self::Infra => Remediation {
                rule: "infra is framework, runtime, and glue with zero domain knowledge; it may depend only on other infra crates.",
                fixes: &[
                    "If this crate needs a domain or adapter type, it isn't infra — retag it for the layer it actually serves.",
                    "If it's domain-aware wiring, move that glue up to the composition-root, which is allowed to know everything.",
                ],
            },
            Self::CompositionRoot => Remediation {
                rule: "the composition-root may depend on every role — it's where wiring lives — so a forbidden edge out of it should be impossible.",
                fixes: &[
                    "If you're seeing this, the role matrix changed or the crate is mislabeled; double-check the role tag, then file a bug.",
                ],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Role;

    const ALL: &[Role] = Role::ALL;

    #[test]
    fn all_lists_every_variant() {
        // Adding a variant makes this match non-exhaustive and the test stops
        // compiling, so extending `Role::ALL` is enforced rather than hoped for.
        const fn tag(r: Role) -> usize {
            match r {
                Role::Domain => 0,
                Role::Usecase => 1,
                Role::PortAndAdapter => 2,
                Role::DrivenAdapter => 3,
                Role::DrivingAdapter => 4,
                Role::Infra => 5,
                Role::CompositionRoot => 6,
            }
        }
        assert_eq!(Role::ALL.len(), tag(Role::CompositionRoot) + 1);
    }

    #[test]
    fn matrix_is_transitively_closed() {
        // The theorem the whole gate rests on: if every *direct* edge in a
        // workspace is legal, every *transitive* edge is legal too. Formally,
        // for every role R and every A in allowed(R), allowed(A) is a subset
        // of allowed(R) — so a dep closure can never reach a role the direct
        // matrix forbids. A separate transitive walk is therefore redundant,
        // and any second implementation of it is drift waiting to happen.
        //
        // Note the one escape hatch: a sanctioned exception deliberately
        // admits an illegal direct edge, and consumers of that crate inherit
        // the hole. Closure is a property of the matrix, not of a workspace
        // that has overridden it.
        for &consumer in Role::ALL {
            for &direct in consumer.allowed_deps() {
                for &indirect in direct.allowed_deps() {
                    assert!(
                        consumer.allowed_deps().contains(&indirect),
                        "closure broken: {consumer:?} -> {direct:?} -> {indirect:?}, \
                         but {consumer:?} may not depend on {indirect:?} directly"
                    );
                }
            }
        }
    }

    #[test]
    fn parse_round_trip() {
        for &r in ALL {
            assert_eq!(
                Role::parse(r.as_str()),
                Some(r),
                "round-trip failed for {r:?}"
            );
        }
    }

    #[test]
    fn parse_unknown() {
        assert_eq!(Role::parse(""), None);
        assert_eq!(Role::parse("DOMAIN"), None);
        assert_eq!(Role::parse("nonsense"), None);
    }

    #[test]
    fn domain_only_depends_on_domain() {
        assert_eq!(Role::Domain.allowed_deps(), &[Role::Domain]);
    }

    #[test]
    fn infra_only_depends_on_infra() {
        assert_eq!(Role::Infra.allowed_deps(), &[Role::Infra]);
    }

    #[test]
    fn composition_root_can_depend_on_anything() {
        let allowed: &[Role] = Role::CompositionRoot.allowed_deps();
        for &r in ALL {
            assert!(allowed.contains(&r), "composition-root should allow {r:?}");
        }
    }

    #[test]
    fn usecase_cannot_depend_on_adapters_or_infra() {
        let allowed: &[Role] = Role::Usecase.allowed_deps();
        assert!(!allowed.contains(&Role::DrivenAdapter));
        assert!(!allowed.contains(&Role::DrivingAdapter));
        assert!(!allowed.contains(&Role::Infra));
        assert!(!allowed.contains(&Role::CompositionRoot));
    }

    #[test]
    fn port_and_adapter_cannot_depend_on_usecase() {
        let allowed: &[Role] = Role::PortAndAdapter.allowed_deps();
        assert!(!allowed.contains(&Role::Usecase));
    }

    #[test]
    fn adapters_are_leaves() {
        // Driven/driving adapters are deliberately NOT allowed to depend on
        // any adapter (themselves or each other). Sharing goes through infra
        // or the composition root.
        for adapter in [Role::DrivenAdapter, Role::DrivingAdapter] {
            let allowed: &[Role] = adapter.allowed_deps();
            assert!(!allowed.contains(&Role::DrivenAdapter), "{adapter:?}");
            assert!(!allowed.contains(&Role::DrivingAdapter), "{adapter:?}");
        }
    }

    #[test]
    fn every_role_has_a_nonempty_rule_and_at_least_one_fix() {
        for &r in ALL {
            let rem = r.remediation();
            assert!(!rem.rule.is_empty(), "{r:?} has empty rule");
            assert!(!rem.fixes.is_empty(), "{r:?} has no fixes");
            for fix in rem.fixes {
                assert!(!fix.is_empty(), "{r:?} has an empty fix");
            }
        }
    }

    #[test]
    fn domain_remediation_points_at_a_port() {
        // The canonical recovery for a pure-layer violation is dependency
        // inversion through a port. If that word ever drops out, the advice
        // has lost its teeth.
        let rem = Role::Domain.remediation();
        assert!(
            rem.fixes.iter().any(|f| f.contains("port")),
            "domain remediation should mention a port: {rem:?}"
        );
    }

    #[test]
    fn nothing_can_depend_on_composition_root_except_itself() {
        // Composition root sits at the top: only it can name itself.
        for &r in ALL {
            if r == Role::CompositionRoot {
                continue;
            }
            assert!(
                !r.allowed_deps().contains(&Role::CompositionRoot),
                "{r:?} should not depend on composition-root"
            );
        }
    }
}
