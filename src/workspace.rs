//! Driven adapter: read a Cargo workspace via `cargo metadata`, extract
//! each member's hex-arch role, and emit the workspace-internal dep edges.
//!
//! This file is the only place that knows the cargo_metadata crate exists.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cargo_metadata::{DependencyKind, MetadataCommand, Package};
use serde::Deserialize;

use crate::lint::{DepEdge, WorkspacePackage};
use crate::role::Role;

/// Snapshot of a Cargo workspace as the lint use case wants to see it.
#[derive(Debug)]
pub struct Workspace {
    pub root: PathBuf,
    pub packages: Vec<WorkspacePackage>,
    pub edges: Vec<DepEdge>,
}

/// Reasons we couldn't produce a `Workspace`.
#[derive(Debug)]
pub enum LoadError {
    /// `cargo metadata` itself failed.
    Metadata(String),
    /// Resolve graph absent (--no-deps or similar).
    NoResolve,
    /// One or more workspace packages have a missing or unparseable role.
    BadRoles(Vec<(String, String)>),
}

#[derive(Deserialize)]
struct HexArchMeta {
    #[serde(rename = "hex-arch")]
    hex_arch: HexArch,
}

#[derive(Deserialize)]
struct HexArch {
    role: String,
    /// The member's declared bounded context, if any. Optional at intake: a
    /// crate that omits it is simply not part of the context axis yet. Read
    /// verbatim, unvalidated — the adoption/partial rule and the reserved
    /// `shared` name live in the context domain, not in this adapter.
    #[serde(default)]
    context: Option<String>,
}

/// Parse one member's `[package.metadata.hex-arch]` intake table: the validated
/// role and the optional, unvalidated bounded-context tag. Policy-free on
/// context — carrying the tag is all this adapter does; the context domain
/// decides what it means.
fn extract_hex_arch(pkg: &Package) -> Result<(Role, Option<String>), String> {
    let meta: HexArchMeta = serde_json::from_value(pkg.metadata.clone())
        .map_err(|_| "missing package.metadata.hex-arch.role".to_owned())?;
    let role: Role = Role::parse(&meta.hex_arch.role)
        .ok_or_else(|| format!("unknown role `{}`", meta.hex_arch.role))?;
    Ok((role, meta.hex_arch.context))
}

pub fn load(manifest_path: Option<&Path>) -> Result<Workspace, LoadError> {
    let mut cmd: MetadataCommand = MetadataCommand::new();
    if let Some(p) = manifest_path {
        cmd.manifest_path(p);
    }
    let metadata = cmd.exec().map_err(|e| LoadError::Metadata(e.to_string()))?;

    let workspace_ids: std::collections::BTreeSet<_> = metadata.workspace_members.iter().collect();

    let mut name_by_id: BTreeMap<&cargo_metadata::PackageId, &str> = BTreeMap::new();
    let mut packages: Vec<WorkspacePackage> = Vec::new();
    let mut bad_roles: Vec<(String, String)> = Vec::new();

    for pkg in &metadata.packages {
        if !workspace_ids.contains(&&pkg.id) {
            continue;
        }
        name_by_id.insert(&pkg.id, pkg.name.as_str());
        match extract_hex_arch(pkg) {
            Ok((role, context)) => packages.push(WorkspacePackage {
                name: pkg.name.to_string(),
                role,
                context,
            }),
            Err(why) => bad_roles.push((pkg.name.to_string(), why)),
        }
    }

    if !bad_roles.is_empty() {
        return Err(LoadError::BadRoles(bad_roles));
    }

    let resolve = metadata.resolve.ok_or(LoadError::NoResolve)?;

    let mut edges: Vec<DepEdge> = Vec::new();
    for node in &resolve.nodes {
        let Some(&consumer_name) = name_by_id.get(&node.id) else {
            continue;
        };
        for dep in &node.deps {
            let Some(&dep_name) = name_by_id.get(&dep.pkg) else {
                continue;
            };
            let is_runtime: bool = dep
                .dep_kinds
                .iter()
                .any(|k| matches!(k.kind, DependencyKind::Normal | DependencyKind::Build));
            if !is_runtime {
                continue;
            }
            edges.push(DepEdge {
                consumer: consumer_name.to_owned(),
                dep: dep_name.to_owned(),
            });
        }
    }

    Ok(Workspace {
        root: PathBuf::from(metadata.workspace_root.as_str()),
        packages,
        edges,
    })
}

#[cfg(test)]
mod tests {
    use super::{HexArch, HexArchMeta};
    use serde_json::{json, Value};

    /// Deserialize a `[package.metadata]` blob the way `cargo metadata` hands it
    /// to us, returning just the `hex-arch` table.
    fn parse(metadata: Value) -> HexArch {
        let meta: HexArchMeta =
            serde_json::from_value(metadata).expect("well-formed hex-arch metadata");
        meta.hex_arch
    }

    #[test]
    fn context_absent_deserializes_to_none() {
        let hex_arch: HexArch = parse(json!({ "hex-arch": { "role": "domain" } }));
        assert_eq!(hex_arch.context, None);
    }

    #[test]
    fn context_present_is_carried_verbatim() {
        let hex_arch: HexArch =
            parse(json!({ "hex-arch": { "role": "domain", "context": "shopping" } }));
        assert_eq!(hex_arch.context, Some("shopping".to_owned()));
    }

    #[test]
    fn reserved_shared_context_is_read_without_interpretation() {
        let hex_arch: HexArch =
            parse(json!({ "hex-arch": { "role": "port-and-adapter", "context": "shared" } }));
        assert_eq!(hex_arch.context, Some("shared".to_owned()));
    }
}
