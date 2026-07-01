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
    /// One-sentence statement of the constraint that was violated.
    pub rule: &'static str,
    /// Concrete fix options, each a complete imperative sentence.
    pub fixes: &'static [&'static str],
}
