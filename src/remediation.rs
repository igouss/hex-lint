//! Domain entity: actionable recovery guidance for a forbidden edge.
//!
//! Pure: no framework, no IO, no allocation beyond `&'static str`. Kept
//! axis-neutral so both the role check and the context check can attach
//! remediation without either axis depending on the other.

/// Actionable recovery guidance for a forbidden edge. The diagnostic alone
/// ("X may not depend on Y") says *what* broke; this says *how* to fix the
/// architecture so a human — or an agent under a "make the build pass"
/// objective — recovers instead of thrashing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Remediation {
    /// Why this layer exists, and what it must never reach for.
    ///
    /// Prose only. It must **not** restate which roles a consumer may depend
    /// on: for the role axis that set is generated from `Role::allowed_deps`
    /// by `Role::allowed_clause`, and a second hand-written copy is drift
    /// waiting to happen. It already happened once — adding `kernel` updated
    /// the matrix and left four role contracts stating a closed list that no
    /// longer held. `role::tests::rationale_never_restates_the_allowed_list`
    /// is the guard.
    ///
    /// Naming a role the consumer may *not* depend on is the point of this
    /// field, and stays welcome.
    pub rationale: &'static str,
    /// Concrete fix options, each a complete imperative sentence.
    pub fixes: &'static [&'static str],
}
