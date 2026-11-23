//! Presentation: the machine-readable contract (`--capabilities`).
//!
//! Pure: builds a value, prints nothing, reads no file, needs no workspace.
//!
//! WHY THIS EXISTS. `--format json` answers *did I violate?* — after the code is
//! written, and only from inside a loadable workspace. This answers the question
//! that comes first: *what am I allowed to do here?* An agent can read the role
//! matrix, the context predicate and the exceptions schema straight out of the
//! tool, before it generates a crate, instead of inferring them from a failure.
//! Catching an inverted dependency is worth less than not generating one.
//!
//! DERIVED, NEVER AUTHORED. Every rule in this document is computed from the
//! domain types that enforce it — [`Role::ALL`] and [`Role::allowed_deps`] for
//! the matrix, [`Role::remediation`] for the per-role contracts, [`context`] for
//! the isolation rule and its reserved name, [`OutsideEveryContext`] for the
//! roles the isolation predicate exempts, [`Rule::ALL`] for the rule
//! list. Nothing here is a second copy of a rule that lives somewhere else, so
//! the document cannot drift away from the check. A hand-written contract is a
//! doc that rots; this one fails to compile instead.

use serde_json::{json, Value};

use crate::context;
use crate::lint::OutsideEveryContext;
use crate::role::Role;
use crate::rule::Rule;

/// Bump when a consumer keying on this document would break. Additive fields
/// are not breaking; renaming or removing one is.
const SCHEMA_VERSION: u32 = 1;

/// The contract as JSON text, pretty-printed. `version` is the tool's own
/// version, passed in so this module carries no build-time coupling.
pub fn render(version: &str) -> String {
    serde_json::to_string_pretty(&document(version))
        .expect("capabilities document is built from owned values and always serializes")
}

/// The contract as a value. Deterministic: same version in, same value out.
pub fn document(version: &str) -> Value {
    json!({
        "tool": "hex-lint",
        "version": version,
        "schema_version": SCHEMA_VERSION,
        "purpose": "Enforce hexagonal-architecture role boundaries — and optional bounded-context isolation — across a Cargo workspace, at crate granularity.",
        "invocation": invocation(),
        "exit_codes": exit_codes(),
        "formats": ["text", "json"],
        "declaration": declaration(),
        "roles": Role::ALL.iter().map(|r: &Role| r.as_str()).collect::<Vec<&'static str>>(),
        "roles_note": "Listed innermost first, then tool, which stands outside that order: it is no part of the running system, and no other role may depend on it. The order is documentary; the matrix below is authoritative.",
        "matrix": matrix(),
        "role_contracts": role_contracts(),
        "context_axis": context_axis(),
        "exceptions": exceptions(),
        "rules": rules(),
        "report_schema": report_schema(),
        "limits": limits(),
    })
}

fn invocation() -> Value {
    json!({
        "check": "hex-lint [--manifest-path <Cargo.toml>] [--exceptions <PATH>] [--format text|json]",
        "contract": "hex-lint --capabilities   (this document; needs no workspace, always exits 0)",
        "explain": "hex-lint explain <ROLE>   (one role's contract and how to fix violations, as prose)",
        "reads": "cargo metadata for the workspace member list and the resolved dependency graph",
        "writes": "nothing — the tool never edits your code or your exceptions file"
    })
}

fn exit_codes() -> Value {
    // Stated honestly: the tool currently has two outcomes, and a usage error is
    // not distinguishable from a violation. An agent that keys on exit 1 as
    // "architecture violation" would be wrong about a bad --manifest-path, so
    // say so here rather than let it infer a third code that does not exist.
    json!({
        "0": "clean — every workspace-internal edge legal or sanctioned, and no stale exceptions",
        "1": "a violation, a stale exception, OR a usage/IO/cargo-metadata error — NOT currently distinguished; parse stdout (--format json emits a report only on a successful run) to tell them apart"
    })
}

fn declaration() -> Value {
    json!({
        "where": "each workspace member's Cargo.toml",
        "toml": "[package.metadata.hex-arch]\nrole = \"domain\"\ncontext = \"shopping\"   # optional",
        "role": "required on every workspace member; missing or unrecognized is a hard error",
        "context": "optional, but all-or-nothing across the workspace (see context_axis.adoption)"
    })
}

/// The role matrix, read straight off the enforcing function.
fn matrix() -> Value {
    let rows: Vec<Value> = Role::ALL
        .iter()
        .map(|&r: &Role| {
            json!({
                "consumer_role": r.as_str(),
                "allowed_deps": r
                    .allowed_deps()
                    .iter()
                    .map(|d: &Role| d.as_str())
                    .collect::<Vec<&'static str>>(),
            })
        })
        .collect();
    Value::Array(rows)
}

/// Per-role constraint statement plus the concrete fixes, so an agent that is
/// about to place a crate can read the rule it is placing it under.
fn role_contracts() -> Value {
    let rows: Vec<Value> = Role::ALL
        .iter()
        .map(|&r: &Role| {
            let rem: crate::remediation::Remediation = r.remediation();
            json!({
                "role": r.as_str(),
                "rule": r.contract(),
                "fixes": rem.fixes,
            })
        })
        .collect();
    Value::Array(rows)
}

fn context_axis() -> Value {
    let rem: crate::remediation::Remediation = context::remediation();
    json!({
        "optional": true,
        "adoption": "all-or-nothing: zero members declare a context (axis off), every member declares one (axis on), or some do and some do not (hard error — partial adoption is meaningless)",
        "reserved_context": context::SHARED,
        "predicate": predicate(),
        "predicate_note": "This subsumes \"shared may depend only on shared\": a shared consumer passes only when the dependency is also shared.",
        "exemption": "A composition-root or tool crate is not judged on this axis: each sits outside every context, a root to wire them together and a tool to read them all. The role matrix forbids every role that lacks that standing from depending on a crate that has it, so the exemption cannot become a way into another context.",
        "orthogonal_to_roles": true,
        "rule": rem.rationale,
        "fixes": rem.fixes,
    })
}

/// The isolation predicate, its exempt roles read off the witness that exempts
/// them.
fn predicate() -> String {
    let exempt: String = Role::ALL
        .iter()
        .filter(|&&r: &&Role| OutsideEveryContext::of(r).is_some())
        .map(|r: &Role| format!("consumer.role == \"{}\" OR ", r.as_str()))
        .collect();
    format!(
        "consumer -> dep passes iff {exempt}consumer.context == dep.context OR dep.context == \"shared\""
    )
}

fn exceptions() -> Value {
    json!({
        "default_path": "<workspace-root>/hex-lint-exceptions.toml",
        "override": "--exceptions <PATH> (an explicitly named file that does not exist is an error; a missing default file means \"no exceptions\")",
        "toml": "[[exception]]\nconsumer = \"my-domain\"\ndep      = \"legacy-helper\"\naxis     = \"role\"      # optional: \"role\" (default) or \"context\"\nticket   = \"ARCH-1234\"\nreason   = \"deleting after the rewrite lands\"",
        "fields": {
            "consumer": "required — the depending crate",
            "dep": "required — the depended-on crate",
            "axis": "optional, \"role\" | \"context\", default \"role\"; an exception only sanctions violations on its own axis",
            "ticket": "required",
            "reason": "required"
        },
        "staleness": "An entry that no longer corresponds to a current violation FAILS the lint. The file cannot rot into a permanent allowlist, and it is the honest record of architectural debt.",
        "agent_guidance": "Do not add an exception to make the build pass. An exception is a declaration of debt with an owner and a ticket; the fixes in role_contracts are the intended response."
    })
}

fn rules() -> Value {
    let rows: Vec<Value> = Rule::ALL
        .iter()
        .map(|&r: &Rule| {
            json!({
                "id": r.id(),
                "axis": r.axis(),
                "description": r.description(),
            })
        })
        .collect();
    Value::Array(rows)
}

/// The shape of `--format json` output, so a consumer can be written against
/// the contract rather than against a sample run.
fn report_schema() -> Value {
    json!({
        "version": "u32 — the report schema version (independent of this document's schema_version)",
        "status": "pass|fail",
        "package_count": "usize",
        "packages": ["name", "role", "context?"],
        "matrix": ["consumer_role", "allowed_deps"],
        "rules": ["id", "description", "status", "failure_count"],
        "violations": [
            "axis", "consumer", "consumer_role", "dep", "dep_role",
            "sanctioned", "ticket?", "reason?", "remediation"
        ],
        "context_violations": [
            "axis", "consumer", "consumer_context", "dep", "dep_context",
            "sanctioned", "ticket?", "reason?", "remediation"
        ],
        "exceptions": ["consumer", "dep", "axis", "ticket", "reason"],
        "stale_exceptions": "same shape as exceptions — role axis",
        "context_stale_exceptions": "same shape as exceptions — context axis",
        "remediation": ["rule", "fixes"],
        "note": "violations lists ALL violations including sanctioned ones (sanctioned: true); status is driven by the unsanctioned subset."
    })
}

/// What a green does NOT mean. An agent trusting this gate should be told its
/// blind spots by the gate itself, not discover them by shipping through one.
fn limits() -> Value {
    json!([
        "Granularity is the CRATE. Mixing roles inside one crate is NOT detected — split it into one crate per role if you want the boundary checked.",
        "Only workspace-internal edges are checked. Third-party dependencies are not roled and never violate the matrix, so a domain crate pulling in a web framework passes this lint (that is effect-audit's axis, not this one).",
        "Runtime and build dependencies are checked; dev-dependencies are not.",
        "A sanctioned exception admits a genuinely illegal edge, and consumers of that crate inherit the hole — matrix transitive closure is a property of the matrix, not of a workspace that has overridden it.",
        "The lint reads the declared role. A crate tagged with the wrong role passes against the wrong contract; nothing here verifies that a crate's tag matches what its code actually does."
    ])
}

#[cfg(test)]
mod tests {
    use super::{document, render, SCHEMA_VERSION};
    use crate::context;
    use crate::lint::OutsideEveryContext;
    use crate::role::Role;
    use crate::rule::Rule;
    use serde_json::Value;

    fn doc() -> Value {
        document("9.9.9")
    }

    #[test]
    fn carries_tool_identity_and_versions() {
        let d: Value = doc();
        assert_eq!(d["tool"], "hex-lint");
        assert_eq!(d["version"], "9.9.9");
        assert_eq!(d["schema_version"], SCHEMA_VERSION);
    }

    #[test]
    fn render_is_parseable_json() {
        let text: String = render("9.9.9");
        let parsed: Value = serde_json::from_str(&text).expect("render emits valid JSON");
        assert_eq!(parsed, doc());
    }

    #[test]
    fn matrix_has_one_row_per_role() {
        let d: Value = doc();
        let rows: &Vec<Value> = d["matrix"].as_array().expect("matrix is an array");
        assert_eq!(rows.len(), Role::ALL.len());
    }

    #[test]
    fn matrix_is_derived_from_the_enforcing_function() {
        // The whole point of this module: the published matrix and the matrix
        // the check runs are the same data. If someone ever hand-writes a row,
        // this fails.
        let d: Value = doc();
        let rows: &Vec<Value> = d["matrix"].as_array().expect("matrix is an array");
        for (row, &role) in rows.iter().zip(Role::ALL) {
            assert_eq!(row["consumer_role"], role.as_str());
            let published: Vec<&str> = row["allowed_deps"]
                .as_array()
                .expect("allowed_deps is an array")
                .iter()
                .map(|v: &Value| v.as_str().expect("dep is a string"))
                .collect();
            let enforced: Vec<&str> = role
                .allowed_deps()
                .iter()
                .map(|d: &Role| d.as_str())
                .collect();
            assert_eq!(published, enforced, "matrix row drifted for {role:?}");
        }
    }

    #[test]
    fn every_role_has_a_published_contract() {
        let d: Value = doc();
        let rows: &Vec<Value> = d["role_contracts"]
            .as_array()
            .expect("role_contracts is an array");
        assert_eq!(rows.len(), Role::ALL.len());
        for (row, &role) in rows.iter().zip(Role::ALL) {
            assert_eq!(row["role"], role.as_str());
            assert_eq!(row["rule"], role.contract());
            assert!(
                !row["fixes"]
                    .as_array()
                    .expect("fixes is an array")
                    .is_empty(),
                "{role:?} published with no fixes"
            );
        }
    }

    #[test]
    fn rules_match_the_enforced_rule_set() {
        let d: Value = doc();
        let rows: &Vec<Value> = d["rules"].as_array().expect("rules is an array");
        assert_eq!(rows.len(), Rule::ALL.len());
        for (row, &rule) in rows.iter().zip(Rule::ALL) {
            assert_eq!(row["id"], rule.id());
            assert_eq!(row["description"], rule.description());
        }
    }

    #[test]
    fn reserved_context_name_is_derived() {
        let d: Value = doc();
        assert_eq!(d["context_axis"]["reserved_context"], context::SHARED);
        assert_eq!(d["context_axis"]["rule"], context::remediation().rationale);
    }

    #[test]
    fn the_context_predicate_publishes_its_exemption() {
        // An agent placing a composition root reads the predicate, not the
        // prose. If the exemption is only in the prose, the formal statement
        // the agent keys on is wrong.
        let d: Value = doc();
        let predicate: &str = d["context_axis"]["predicate"]
            .as_str()
            .expect("predicate is a string");
        let unnamed: Vec<&'static str> = Role::ALL
            .iter()
            .filter(|&&r: &&Role| OutsideEveryContext::of(r).is_some())
            .map(|&r: &Role| r.as_str())
            .filter(|name: &&str| !predicate.contains(&format!("consumer.role == \"{name}\"")))
            .collect();
        assert_eq!(unnamed, Vec::<&'static str>::new());
        assert!(
            d["context_axis"]["exemption"].is_string(),
            "the exemption must be published as its own field"
        );
    }

    #[test]
    fn exit_codes_do_not_promise_a_code_the_tool_cannot_emit() {
        // main() returns only ExitCode::SUCCESS or ExitCode::FAILURE. Publishing
        // a "2" here would be a fabricated contract.
        let d: Value = doc();
        let codes: &serde_json::Map<String, Value> = d["exit_codes"]
            .as_object()
            .expect("exit_codes is an object");
        assert_eq!(codes.len(), 2);
        assert!(codes.contains_key("0") && codes.contains_key("1"));
    }

    #[test]
    fn limits_are_published() {
        // A contract that lists only what the gate catches invites the false
        // green. Non-empty is the assertion; the wording is prose.
        let d: Value = doc();
        assert!(!d["limits"]
            .as_array()
            .expect("limits is an array")
            .is_empty());
    }
}
