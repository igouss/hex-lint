//! hex-lint — workspace lint that enforces hexagonal-architecture role
//! boundaries declared via `package.metadata.hex-arch.role` in each member's
//! `Cargo.toml`.
//!
//! The lint fails on:
//!
//! 1. Any workspace package missing or carrying an unrecognized `role`.
//! 2. Any role-matrix violation not listed in the exceptions file.
//! 3. Any exceptions-file entry that no longer corresponds to a real
//!    violation — keeps the file honest as debt is paid off.

#![allow(clippy::print_stderr, reason = "this IS a CLI tool")]
#![allow(clippy::print_stdout, reason = "this IS a CLI tool")]

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cargo_metadata::{DependencyKind, MetadataCommand, Package, PackageId};
use serde::Deserialize;

const DEFAULT_EXCEPTIONS_FILENAME: &str = "hex-lint-exceptions.toml";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Role {
    Domain,
    Usecase,
    PortAndAdapter,
    DrivenAdapter,
    DrivingAdapter,
    Infra,
    CompositionRoot,
}

impl Role {
    fn parse(s: &str) -> Option<Self> {
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

    fn as_str(self) -> &'static str {
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
    fn allowed_deps(self) -> &'static [Self] {
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

#[derive(Debug, Deserialize)]
struct ExceptionsFile {
    #[serde(rename = "exception", default)]
    exceptions: Vec<Exception>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct Exception {
    consumer: String,
    dep: String,
    #[allow(dead_code, reason = "ticket+reason are documentation, not lint logic")]
    ticket: String,
    #[allow(dead_code, reason = "ticket+reason are documentation, not lint logic")]
    reason: String,
}

struct Args {
    exceptions: Option<PathBuf>,
    manifest_path: Option<PathBuf>,
}

fn print_help() {
    println!("hex-lint {}", env!("CARGO_PKG_VERSION"));
    println!("Enforce hexagonal-architecture role boundaries across a Cargo workspace.");
    println!();
    println!("USAGE:");
    println!("    hex-lint [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!(
        "    -e, --exceptions <PATH>   Exceptions TOML. Default: <workspace-root>/{DEFAULT_EXCEPTIONS_FILENAME}"
    );
    println!("        --manifest-path <PATH>  Path to a Cargo.toml in the workspace.");
    println!("    -h, --help                Print this help.");
    println!("    -V, --version             Print version.");
    println!();
    println!("ROLES:");
    println!("    domain, usecase, port-and-adapter, driven-adapter,");
    println!("    driving-adapter, infra, composition-root");
    println!();
    println!("Tag each workspace package's Cargo.toml:");
    println!("    [package.metadata.hex-arch]");
    println!("    role = \"domain\"");
}

fn parse_args() -> Result<Args, String> {
    let mut it = env::args().skip(1);
    let mut exceptions: Option<PathBuf> = None;
    let mut manifest_path: Option<PathBuf> = None;
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("hex-lint {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-e" | "--exceptions" => {
                let v: String = it
                    .next()
                    .ok_or_else(|| "--exceptions requires a value".to_string())?;
                exceptions = Some(PathBuf::from(v));
            }
            "--manifest-path" => {
                let v: String = it
                    .next()
                    .ok_or_else(|| "--manifest-path requires a value".to_string())?;
                manifest_path = Some(PathBuf::from(v));
            }
            s if s.starts_with("--exceptions=") => {
                exceptions = Some(PathBuf::from(s.trim_start_matches("--exceptions=")));
            }
            s if s.starts_with("--manifest-path=") => {
                manifest_path = Some(PathBuf::from(s.trim_start_matches("--manifest-path=")));
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        exceptions,
        manifest_path,
    })
}

fn extract_role(pkg: &Package) -> Result<Role, String> {
    let raw: Option<&str> = pkg
        .metadata
        .as_object()
        .and_then(|m| m.get("hex-arch"))
        .and_then(serde_json::Value::as_object)
        .and_then(|m| m.get("role"))
        .and_then(serde_json::Value::as_str);
    match raw {
        None => Err("missing package.metadata.hex-arch.role".to_string()),
        Some(s) => Role::parse(s).ok_or_else(|| format!("unknown role `{s}`")),
    }
}

fn main() -> ExitCode {
    let args: Args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("hex-lint: {e}");
            eprintln!("run `hex-lint --help` for usage");
            return ExitCode::FAILURE;
        }
    };

    let mut cmd = MetadataCommand::new();
    if let Some(ref p) = args.manifest_path {
        cmd.manifest_path(p);
    }
    let metadata = match cmd.exec() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("hex-lint: cargo metadata failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    let workspace_ids: BTreeSet<&PackageId> = metadata.workspace_members.iter().collect();

    let mut packages_by_id: BTreeMap<&PackageId, &Package> = BTreeMap::new();
    let mut roles: BTreeMap<String, Result<Role, String>> = BTreeMap::new();
    for pkg in &metadata.packages {
        if !workspace_ids.contains(&pkg.id) {
            continue;
        }
        packages_by_id.insert(&pkg.id, pkg);
        roles.insert(pkg.name.clone(), extract_role(pkg));
    }

    // Fail fast on bad roles (no point checking deps if metadata is wrong).
    let bad_roles: Vec<(&String, &String)> = roles
        .iter()
        .filter_map(|(n, r)| r.as_ref().err().map(|e| (n, e)))
        .collect();
    if !bad_roles.is_empty() {
        eprintln!("hex-lint: workspace packages with bad role:");
        for (name, why) in bad_roles {
            eprintln!("  {name}: {why}");
        }
        return ExitCode::FAILURE;
    }

    let resolve = match metadata.resolve {
        Some(r) => r,
        None => {
            eprintln!("hex-lint: cargo metadata returned no resolve graph");
            return ExitCode::FAILURE;
        }
    };

    // Enumerate runtime/build workspace-internal violations.
    let mut violations: Vec<(String, Role, String, Role)> = Vec::new();
    for node in &resolve.nodes {
        if !workspace_ids.contains(&node.id) {
            continue;
        }
        let consumer_pkg = packages_by_id[&node.id];
        let consumer_role = match roles[&consumer_pkg.name] {
            Ok(r) => r,
            Err(_) => unreachable!("bad roles caused early return above"),
        };

        for dep in &node.deps {
            if !workspace_ids.contains(&dep.pkg) {
                continue;
            }
            let dep_pkg = packages_by_id[&dep.pkg];
            let dep_role = match roles[&dep_pkg.name] {
                Ok(r) => r,
                Err(_) => unreachable!("bad roles caused early return above"),
            };

            let is_runtime = dep
                .dep_kinds
                .iter()
                .any(|k| matches!(k.kind, DependencyKind::Normal | DependencyKind::Build));
            if !is_runtime {
                continue;
            }

            if !consumer_role.allowed_deps().contains(&dep_role) {
                violations.push((
                    consumer_pkg.name.clone(),
                    consumer_role,
                    dep_pkg.name.clone(),
                    dep_role,
                ));
            }
        }
    }

    // Resolve exceptions path. Explicit --exceptions must exist; default path
    // is optional (treat missing as "no exceptions").
    let (exceptions_path, exceptions_required): (PathBuf, bool) = match args.exceptions {
        Some(p) => (p, true),
        None => (
            PathBuf::from(metadata.workspace_root.as_str()).join(DEFAULT_EXCEPTIONS_FILENAME),
            false,
        ),
    };

    let exceptions: Vec<Exception> = if Path::new(&exceptions_path).exists() {
        let raw = match std::fs::read_to_string(&exceptions_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("hex-lint: cannot read {}: {e}", exceptions_path.display());
                return ExitCode::FAILURE;
            }
        };
        match toml::from_str::<ExceptionsFile>(&raw) {
            Ok(f) => f.exceptions,
            Err(e) => {
                eprintln!("hex-lint: cannot parse {}: {e}", exceptions_path.display());
                return ExitCode::FAILURE;
            }
        }
    } else if exceptions_required {
        eprintln!(
            "hex-lint: exceptions file not found: {}",
            exceptions_path.display()
        );
        return ExitCode::FAILURE;
    } else {
        Vec::new()
    };

    let viol_keys: BTreeSet<(String, String)> = violations
        .iter()
        .map(|(c, _, d, _)| (c.clone(), d.clone()))
        .collect();
    let exc_keys: BTreeSet<(String, String)> = exceptions
        .iter()
        .map(|e| (e.consumer.clone(), e.dep.clone()))
        .collect();

    let unsanctioned: BTreeSet<&(String, String)> = viol_keys.difference(&exc_keys).collect();
    let stale: BTreeSet<&(String, String)> = exc_keys.difference(&viol_keys).collect();

    let mut had_problem = false;

    if !unsanctioned.is_empty() {
        eprintln!("hex-lint: unsanctioned hex-arch role violations (not in exceptions file):");
        for (cn, dn) in &unsanctioned {
            let cr = roles[cn].as_ref().expect("bad roles already short-circuit");
            let dr = roles[dn].as_ref().expect("bad roles already short-circuit");
            eprintln!(
                "  {cn} ({}) -> {dn} ({}): forbidden",
                cr.as_str(),
                dr.as_str()
            );
        }
        had_problem = true;
    }

    if !stale.is_empty() {
        eprintln!("hex-lint: exceptions file entries that no longer match a real violation:");
        eprintln!("(remove them — debt paid off?)");
        for (cn, dn) in &stale {
            eprintln!("  {cn} -> {dn}");
        }
        had_problem = true;
    }

    if had_problem {
        ExitCode::FAILURE
    } else {
        println!(
            "hex-lint: clean ({} workspace packages, {} active violation(s) all sanctioned by {} exception(s))",
            roles.len(),
            violations.len(),
            exceptions.len()
        );
        ExitCode::SUCCESS
    }
}
