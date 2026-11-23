//! Domain entity: hexagonal-architecture role.
//!
//! Pure: no framework, no IO, no allocation beyond `&'static str`.

use crate::remediation::Remediation;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Role {
    Kernel,
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
        Self::Kernel,
        Self::Domain,
        Self::Usecase,
        Self::PortAndAdapter,
        Self::DrivenAdapter,
        Self::DrivingAdapter,
        Self::Infra,
        Self::CompositionRoot,
    ];

    /// Every role name, comma-separated, in outward order.
    ///
    /// For error messages that offer the valid set. Generated from [`Self::ALL`]
    /// so a role added to the enum cannot go missing from the message that is
    /// supposed to list it — the same reason [`Self::allowed_clause`] exists.
    pub fn all_names() -> String {
        Self::ALL
            .iter()
            .map(|r: &Role| r.as_str())
            .collect::<Vec<&'static str>>()
            .join(", ")
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "kernel" => Self::Kernel,
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
            Self::Kernel => "kernel",
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
            CompositionRoot, Domain, DrivenAdapter, DrivingAdapter, Infra, Kernel, PortAndAdapter,
            Usecase,
        };
        match self {
            Kernel => &[Kernel],
            Domain => &[Kernel, Domain],
            Usecase => &[Kernel, Domain, Usecase, PortAndAdapter],
            PortAndAdapter => &[Kernel, Domain, PortAndAdapter],
            DrivenAdapter => &[Kernel, Domain, PortAndAdapter, Infra],
            DrivingAdapter => &[Kernel, Domain, Usecase, PortAndAdapter],
            Infra => &[Infra],
            CompositionRoot => &[
                Kernel,
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

    /// This role's matrix row, rendered as a comma-separated list.
    ///
    /// The single place the allowed set becomes words. Every surface that
    /// shows a user which roles are permitted — `explain`, the violation
    /// diagnostic, the JSON report, the published contract — reads this, so
    /// none of them can disagree with the matrix they are describing.
    pub fn allowed_clause(self) -> String {
        let names: Vec<&'static str> = self
            .allowed_deps()
            .iter()
            .map(|r: &Role| r.as_str())
            .collect();
        names.join(", ")
    }

    /// This role's complete contract in one string: the generated allowed set,
    /// then the hand-written rationale. For callers that want the whole rule as
    /// a single sentence — the JSON report and the published contract — instead
    /// of the two-line layout the text output uses.
    pub fn contract(self) -> String {
        format!(
            "may depend on: {} — and nothing else. {}",
            self.allowed_clause(),
            self.remediation().rationale
        )
    }

    /// Recovery guidance for a forbidden dependency *out of* a crate with this
    /// role. Pure static advice; total over every role (composition-root can't
    /// actually produce a violation, but the function still answers).
    ///
    /// The rationale says *why*; it never enumerates the allowed set. See
    /// [`Remediation::rationale`] and [`Role::allowed_clause`].
    pub fn remediation(self) -> Remediation {
        match self {
            Self::Kernel => Remediation {
                rationale: "kernel is the floor of the system: the shared vocabulary every layer above speaks. It cannot name anything above it — not even domain.",
                fixes: &[
                    "If this crate needs a domain type, the dependency is upside down: the kernel cannot know its consumers. Move the shared type down into the kernel, or retag this crate `domain`.",
                    "If you need behavior rather than vocabulary, declare it as a port (trait) here and let a layer above implement it — the kernel names the abstraction, never the implementation.",
                ],
            },
            Self::Domain => Remediation {
                rationale: "domain is the pure heart of the system: no frameworks, no I/O, no adapters.",
                fixes: &[
                    "If this crate isn't really pure domain, it's mislabeled — give it the role it actually plays (usecase, infra, or an adapter).",
                    "If domain genuinely needs a behavior from outside, invert it: declare a port (trait) in a port-and-adapter crate and depend on the trait, never the implementation.",
                ],
            },
            Self::Usecase => Remediation {
                rationale: "a usecase orchestrates application behavior and reaches the outside world only through ports — never an adapter or infra crate directly.",
                fixes: &[
                    "Declare the capability you need as a port (trait) in a port-and-adapter crate and depend on that; a driven-adapter implements it.",
                    "Let the composition-root inject the concrete implementation — the usecase only ever names the trait.",
                ],
            },
            Self::PortAndAdapter => Remediation {
                rationale: "a port-and-adapter crate holds the trait definitions — the ports — and the plain types they speak in. It cannot reach up into a usecase, an adapter, or infra.",
                fixes: &[
                    "If you're reaching for a usecase or an adapter, this crate is mis-scoped — split the offending code out into the layer that owns it.",
                    "If the type you need is a plain data type, it belongs in domain — move it there and depend on domain.",
                ],
            },
            Self::DrivenAdapter => Remediation {
                rationale: "a driven-adapter implements a port against real infrastructure. It cannot reach up into a usecase, and it cannot reach sideways into another adapter: adapters are leaves.",
                fixes: &[
                    "If two adapters need to share code, extract it into an infra crate they both depend on.",
                    "If this is genuine wiring between collaborators, it belongs in the composition-root, not inside the adapter.",
                ],
            },
            Self::DrivingAdapter => Remediation {
                rationale: "a driving-adapter turns external input into a usecase call. It cannot depend on a driven-adapter or on infra directly, and it cannot reach sideways into another adapter: adapters are leaves.",
                fixes: &[
                    "If two driving adapters share code — argument parsing, rendering, a transport helper — extract it into an infra crate they both depend on.",
                    "Whatever infra or driven-adapter you're reaching for should be injected by the composition-root, already wired.",
                    "If you're tempted to instantiate an adapter here, that construction belongs in the composition-root.",
                ],
            },
            Self::Infra => Remediation {
                rationale: "infra is framework, runtime, and glue with zero domain knowledge.",
                fixes: &[
                    "If this crate needs a domain or adapter type, it isn't infra — retag it for the layer it actually serves.",
                    "If it's domain-aware wiring, move that glue up to the composition-root, which is allowed to know everything.",
                ],
            },
            Self::CompositionRoot => Remediation {
                rationale: "the composition-root is where wiring lives: it is the one place allowed to name every role, so a forbidden edge out of it should be impossible.",
                fixes: &[
                    "If you're seeing this, the role matrix changed or the crate is mislabeled; double-check the role tag, then file a bug.",
                ],
            },
        }
    }
}

#[cfg(test)]
mod tests;
