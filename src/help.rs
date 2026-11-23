use crate::context;
use crate::lint::OutsideEveryContext;
use crate::remediation::Remediation;
use crate::role::Role;
use crate::DEFAULT_EXCEPTIONS_FILENAME;

pub fn print_help() {
    print!("{}", help_text());
}

/// The `--help` text. Pure (returns the string rather than printing it) so a
/// test can assert it keeps documenting every axis the tool actually enforces —
/// the context axis silently outran this text once, and a regression test is
/// cheaper than noticing again.
pub fn help_text() -> String {
    let roles: String = Role::ALL
        .iter()
        .map(|r: &Role| r.as_str())
        .collect::<Vec<&'static str>>()
        .join(", ");
    format!(
        "hex-lint {version}
Enforce hexagonal-architecture role boundaries — and optional bounded-context
isolation — across a Cargo workspace.

USAGE:
    hex-lint [OPTIONS]
    hex-lint explain <ROLE>   Print a role's contract and how to fix violations.

OPTIONS:
    -e, --exceptions <PATH>   Exceptions TOML. Default: <workspace-root>/{DEFAULT_EXCEPTIONS_FILENAME}
        --manifest-path <PATH>  Path to a Cargo.toml in the workspace.
    -f, --format <FMT>        Output format: text (default) or json.
        --capabilities        Print the machine-readable contract as JSON, then
                              exit 0. Needs no workspace — read the role matrix,
                              the context predicate and the exceptions schema
                              BEFORE writing code, not after failing on it.
    -h, --help                Print this help.
    -V, --version             Print version.

ROLES (innermost first; `tool` last, outside that order):
    {roles}

Tag each workspace package's Cargo.toml:
    [package.metadata.hex-arch]
    role = \"domain\"
    context = \"shopping\"   # optional; see CONTEXT ISOLATION

ROLE AXIS:
    Roles are enforced at CRATE granularity. Each workspace member gets one
    role and the crate dependency graph is checked against the matrix. Mixing
    roles inside a single crate is NOT enforced — split it into one crate per
    role if you want the boundary checked.

CONTEXT ISOLATION (optional, orthogonal to roles):
    Give each crate a `context` and hex-lint also enforces bounded-context
    isolation over the same edges: `consumer -> dep` passes iff
    consumer.context == dep.context OR dep.context == \"shared\". \"shared\" is
    the one reserved name — any crate may depend on it; it may depend only on
    itself. Every other name is free-form.

    A `composition-root` crate is exempt: it sits outside every context in
    order to wire them together, so it may name crates in any of them. No other
    role may depend on a composition root, so the exemption opens no way into a
    context.

    Opt-in and all-or-nothing: zero contexts = axis off (roles only); a context
    on every member = axis on; a context on only SOME members is a hard error
    (partial adoption) that exits non-zero before any check runs.

EXCEPTIONS:
    Grandfathered debt lives in the exceptions TOML, one entry per edge, tagged
    axis = \"role\" (default) or axis = \"context\". A stale entry that matches no
    real violation fails the lint on its own axis.
",
        version = env!("CARGO_PKG_VERSION"),
    )
}

pub fn print_explain(role: Role) {
    let rem: Remediation = role.remediation();

    println!("hex-lint — role `{}`", role.as_str());
    println!();
    println!("May depend on: {}", role.allowed_clause());
    println!();
    println!("Contract:");
    println!("    {}", rem.rationale);
    if let Some(witness) = OutsideEveryContext::of(role) {
        print_context_exemption(witness);
    }
    println!();
    println!(
        "If hex-lint flags a forbidden dependency out of a `{}` crate:",
        role.as_str()
    );
    for fix in rem.fixes {
        println!("  - {fix}");
    }
}

/// What the context axis says to a crate that stands outside every context.
/// Takes the witness, so `explain` cannot print this under a role that has not
/// earned it.
fn print_context_exemption(witness: OutsideEveryContext) {
    println!();
    println!("Context isolation:");
    println!("    {}", context::exemption_note(witness));
}
