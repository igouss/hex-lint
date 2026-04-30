//! Domain entity: hexagonal-architecture role.
//!
//! Pure: no framework, no IO, no allocation beyond `&'static str`.

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
}

#[cfg(test)]
mod tests {
    use super::Role;

    const ALL: &[Role] = &[
        Role::Domain,
        Role::Usecase,
        Role::PortAndAdapter,
        Role::DrivenAdapter,
        Role::DrivingAdapter,
        Role::Infra,
        Role::CompositionRoot,
    ];

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
