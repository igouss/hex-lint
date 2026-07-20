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
//!
//! This file is the composition root: it parses args, calls the adapters,
//! runs the use case, and formats output. No business logic lives here.

#![allow(clippy::print_stderr, reason = "this IS a CLI tool")]
#![allow(clippy::print_stdout, reason = "this IS a CLI tool")]

mod context;
mod context_check;
mod exceptions;
mod lint;
mod remediation;
mod role;
mod role_check;
mod workspace;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;

use crate::lint::{Axis, Exception};
use crate::remediation::Remediation;
use crate::role::Role;

const DEFAULT_EXCEPTIONS_FILENAME: &str = "hex-lint-exceptions.toml";

#[derive(Clone, Copy, Eq, PartialEq)]
enum Format {
    Text,
    Json,
}

struct Args {
    exceptions: Option<PathBuf>,
    manifest_path: Option<PathBuf>,
    format: Format,
}

#[cfg_attr(test, derive(Debug))]
enum ParseOutcome {
    Run(Args),
    Explain(Role),
    HelpRequested,
    VersionRequested,
}

#[cfg(test)]
impl std::fmt::Debug for Args {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Args")
            .field("exceptions", &self.exceptions)
            .field("manifest_path", &self.manifest_path)
            .field("format", &self.format.as_str())
            .finish()
    }
}

#[cfg(test)]
impl std::fmt::Debug for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Format {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

#[cfg(test)]
impl Format {
    fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
        }
    }
}

fn print_help() {
    print!("{}", help_text());
}

/// The `--help` text. Pure (returns the string rather than printing it) so a
/// test can assert it keeps documenting every axis the tool actually enforces —
/// the context axis silently outran this text once, and a regression test is
/// cheaper than noticing again.
fn help_text() -> String {
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
    -h, --help                Print this help.
    -V, --version             Print version.

ROLES:
    domain, usecase, port-and-adapter, driven-adapter,
    driving-adapter, infra, composition-root

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

fn print_explain(role: Role) {
    let rem: Remediation = role.remediation();
    let allowed: Vec<&'static str> = role.allowed_deps().iter().map(|r| r.as_str()).collect();

    println!("hex-lint — role `{}`", role.as_str());
    println!();
    println!("May depend on: {}", allowed.join(", "));
    println!();
    println!("Contract:");
    println!("    {}", rem.rule);
    println!();
    println!(
        "If hex-lint flags a forbidden dependency out of a `{}` crate:",
        role.as_str()
    );
    for fix in rem.fixes {
        println!("  - {fix}");
    }
}

fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<ParseOutcome, String> {
    let mut it = args.into_iter();
    let mut exceptions: Option<PathBuf> = None;
    let mut manifest_path: Option<PathBuf> = None;
    let mut format: Format = Format::Text;

    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(ParseOutcome::HelpRequested),
            "-V" | "--version" => return Ok(ParseOutcome::VersionRequested),
            "explain" => {
                let v: String = it
                    .next()
                    .ok_or_else(|| "explain requires a role name".to_owned())?;
                let role: Role = Role::parse(&v)
                    .ok_or_else(|| format!("unknown role `{v}` — see `hex-lint --help`"))?;
                return Ok(ParseOutcome::Explain(role));
            }
            "-e" | "--exceptions" => {
                let v: String = it
                    .next()
                    .ok_or_else(|| "--exceptions requires a value".to_owned())?;
                exceptions = Some(PathBuf::from(v));
            }
            "--manifest-path" => {
                let v: String = it
                    .next()
                    .ok_or_else(|| "--manifest-path requires a value".to_owned())?;
                manifest_path = Some(PathBuf::from(v));
            }
            "-f" | "--format" => {
                let v: String = it
                    .next()
                    .ok_or_else(|| "--format requires a value".to_owned())?;
                format =
                    Format::parse(&v).ok_or_else(|| format!("unknown format `{v}` (text|json)"))?;
            }
            other => {
                if let Some(v) = other.strip_prefix("--exceptions=") {
                    exceptions = Some(PathBuf::from(v));
                } else if let Some(v) = other.strip_prefix("--manifest-path=") {
                    manifest_path = Some(PathBuf::from(v));
                } else if let Some(v) = other.strip_prefix("--format=") {
                    format = Format::parse(v)
                        .ok_or_else(|| format!("unknown format `{v}` (text|json)"))?;
                } else {
                    return Err(format!("unknown argument: {other}"));
                }
            }
        }
    }
    Ok(ParseOutcome::Run(Args {
        exceptions,
        manifest_path,
        format,
    }))
}

fn main() -> ExitCode {
    let args: Args = match parse_args(env::args().skip(1)) {
        Ok(ParseOutcome::Run(a)) => a,
        Ok(ParseOutcome::Explain(role)) => {
            print_explain(role);
            return ExitCode::SUCCESS;
        }
        Ok(ParseOutcome::HelpRequested) => {
            print_help();
            return ExitCode::SUCCESS;
        }
        Ok(ParseOutcome::VersionRequested) => {
            println!("hex-lint {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("hex-lint: {e}");
            eprintln!("run `hex-lint --help` for usage");
            return ExitCode::FAILURE;
        }
    };

    let ws: workspace::Workspace = match workspace::load(args.manifest_path.as_deref()) {
        Ok(w) => w,
        Err(workspace::LoadError::Metadata(e)) => {
            eprintln!("hex-lint: cargo metadata failed: {e}");
            return ExitCode::FAILURE;
        }
        Err(workspace::LoadError::NoResolve) => {
            eprintln!("hex-lint: cargo metadata returned no resolve graph");
            return ExitCode::FAILURE;
        }
        Err(workspace::LoadError::BadRoles(bad)) => {
            eprintln!("hex-lint: workspace packages with bad role:");
            for (name, why) in &bad {
                eprintln!("  {name}: {why}");
            }
            return ExitCode::FAILURE;
        }
    };

    // Resolve context-axis adoption before running any check. Partial adoption
    // (some members declare a context, some do not) is meaningless, so abort
    // hard — mirroring the missing/bad-role abort above — and name the crates
    // that still lack a context.
    let adoption: context::Adoption = context::adoption(&ws.packages);
    if let context::Adoption::Partial(missing) = &adoption {
        eprintln!(
            "hex-lint: context declared on some but not all workspace packages (partial adoption is a hard error); these lack context:"
        );
        for name in missing {
            eprintln!("  {name}");
        }
        return ExitCode::FAILURE;
    }
    let context_enabled: bool = matches!(adoption, context::Adoption::Enabled);

    // Resolve exceptions path. Explicit --exceptions: missing file is an
    // error. Default path (workspace root): missing file means "no exceptions".
    let exceptions_required: bool = args.exceptions.is_some();
    let exceptions_path: PathBuf = args
        .exceptions
        .unwrap_or_else(|| ws.root.join(DEFAULT_EXCEPTIONS_FILENAME));

    let exceptions: Vec<lint::Exception> = match exceptions::load(&exceptions_path) {
        Ok(v) => v,
        Err(exceptions::LoadError::NotFound) if !exceptions_required => Vec::new(),
        Err(exceptions::LoadError::NotFound) => {
            eprintln!(
                "hex-lint: exceptions file not found: {}",
                exceptions_path.display()
            );
            return ExitCode::FAILURE;
        }
        Err(exceptions::LoadError::Io(e)) => {
            eprintln!("hex-lint: cannot read {}: {e}", exceptions_path.display());
            return ExitCode::FAILURE;
        }
        Err(exceptions::LoadError::Parse(e)) => {
            eprintln!("hex-lint: cannot parse {}: {e}", exceptions_path.display());
            return ExitCode::FAILURE;
        }
    };

    let role_report: lint::AxisReport<role_check::RoleViolation> =
        role_check::run(&ws.packages, &ws.edges, &exceptions);
    let context_report: lint::AxisReport<context_check::ContextViolation> =
        context_check::run(&ws.packages, &ws.edges, &exceptions);
    let had_problem: bool = !role_report.unsanctioned.is_empty()
        || !role_report.stale_exceptions.is_empty()
        || !context_report.unsanctioned.is_empty()
        || !context_report.stale_exceptions.is_empty();

    match args.format {
        Format::Text => render_text(
            &ws.packages,
            &role_report,
            &context_report,
            &exceptions,
            context_enabled,
            had_problem,
        ),
        Format::Json => render_json(
            &ws.packages,
            &role_report,
            &context_report,
            &exceptions,
            had_problem,
        ),
    }

    if had_problem {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn render_text(
    packages: &[lint::WorkspacePackage],
    role_report: &lint::AxisReport<role_check::RoleViolation>,
    context_report: &lint::AxisReport<context_check::ContextViolation>,
    exceptions: &[Exception],
    context_enabled: bool,
    had_problem: bool,
) {
    if !role_report.unsanctioned.is_empty() {
        eprintln!("hex-lint: unsanctioned hex-arch role violations (not in exceptions file):");
        for v in &role_report.unsanctioned {
            eprintln!(
                "  {} ({}) -> {} ({}): forbidden",
                v.consumer,
                v.consumer_role.as_str(),
                v.dep,
                v.dep_role.as_str()
            );
            let rem: Remediation = v.consumer_role.remediation();
            eprintln!("    why: {}", rem.rule);
            for fix in rem.fixes {
                eprintln!("    fix: {fix}");
            }
        }
        eprintln!(
            "    (run `hex-lint explain {}` for the full role contract)",
            role_report.unsanctioned[0].consumer_role.as_str()
        );
    }

    if !role_report.stale_exceptions.is_empty() {
        eprintln!("hex-lint: exceptions file entries that no longer match a real violation:");
        eprintln!("(remove them — debt paid off?)");
        for e in &role_report.stale_exceptions {
            eprintln!(
                "  {} -> {}  [ticket={} reason={}]",
                e.consumer, e.dep, e.ticket, e.reason
            );
        }
    }

    if !context_report.unsanctioned.is_empty() {
        eprintln!("hex-lint: unsanctioned context-isolation violations (not in exceptions file):");
        for v in &context_report.unsanctioned {
            eprintln!(
                "  {} [{}] -> {} [{}]: forbidden",
                v.consumer, v.consumer_context, v.dep, v.dep_context
            );
        }
        let rem: Remediation = context::remediation();
        eprintln!("    why: {}", rem.rule);
        for fix in rem.fixes {
            eprintln!("    fix: {fix}");
        }
    }

    if !context_report.stale_exceptions.is_empty() {
        eprintln!(
            "hex-lint: context-axis exceptions file entries that no longer match a real violation:"
        );
        eprintln!("(remove them — debt paid off?)");
        for e in &context_report.stale_exceptions {
            eprintln!(
                "  {} -> {}  [ticket={} reason={}]",
                e.consumer, e.dep, e.ticket, e.reason
            );
        }
    }

    if !had_problem {
        println!(
            "hex-lint: clean ({} workspace packages, {} active violation(s) all sanctioned by {} exception(s))",
            packages.len(),
            role_report.violations.len(),
            exceptions.len()
        );
        if context_enabled {
            let context_exception_count: usize = exceptions
                .iter()
                .filter(|e: &&Exception| e.axis == Axis::Context)
                .count();
            println!(
                "hex-lint: context isolation clean ({} active violation(s) all sanctioned by {} exception(s))",
                context_report.violations.len(),
                context_exception_count
            );
        }
    }
}

fn render_json(
    packages: &[lint::WorkspacePackage],
    role_report: &lint::AxisReport<role_check::RoleViolation>,
    context_report: &lint::AxisReport<context_check::ContextViolation>,
    exceptions: &[Exception],
    had_problem: bool,
) {
    // The exception->violation join is axis-aware: a role exception may only
    // annotate a role violation, a context exception only a context violation.
    let role_exc_by_key: std::collections::BTreeMap<(&str, &str), &Exception> = exceptions
        .iter()
        .filter(|e: &&Exception| e.axis == Axis::Role)
        .map(|e: &Exception| ((e.consumer.as_str(), e.dep.as_str()), e))
        .collect();
    let context_exc_by_key: std::collections::BTreeMap<(&str, &str), &Exception> = exceptions
        .iter()
        .filter(|e: &&Exception| e.axis == Axis::Context)
        .map(|e: &Exception| ((e.consumer.as_str(), e.dep.as_str()), e))
        .collect();

    let violations: Vec<JsonViolation<'_>> = role_report
        .violations
        .iter()
        .map(|v: &role_check::RoleViolation| {
            let exc: Option<&&Exception> =
                role_exc_by_key.get(&(v.consumer.as_str(), v.dep.as_str()));
            let rem: Remediation = v.consumer_role.remediation();
            JsonViolation {
                axis: Axis::Role,
                consumer: &v.consumer,
                consumer_role: v.consumer_role.as_str(),
                dep: &v.dep,
                dep_role: v.dep_role.as_str(),
                sanctioned: exc.is_some(),
                ticket: exc.map(|e: &&Exception| e.ticket.as_str()),
                reason: exc.map(|e: &&Exception| e.reason.as_str()),
                remediation: JsonRemediation {
                    rule: rem.rule,
                    fixes: rem.fixes,
                },
            }
        })
        .collect();

    let context_rem: Remediation = context::remediation();
    let context_violations: Vec<JsonContextViolation<'_>> = context_report
        .violations
        .iter()
        .map(|v: &context_check::ContextViolation| {
            let exc: Option<&&Exception> =
                context_exc_by_key.get(&(v.consumer.as_str(), v.dep.as_str()));
            JsonContextViolation {
                axis: Axis::Context,
                consumer: &v.consumer,
                consumer_context: &v.consumer_context,
                dep: &v.dep,
                dep_context: &v.dep_context,
                sanctioned: exc.is_some(),
                ticket: exc.map(|e: &&Exception| e.ticket.as_str()),
                reason: exc.map(|e: &&Exception| e.reason.as_str()),
                remediation: JsonRemediation {
                    rule: context_rem.rule,
                    fixes: context_rem.fixes,
                },
            }
        })
        .collect();

    let stale_exceptions: Vec<JsonException<'_>> = role_report
        .stale_exceptions
        .iter()
        .map(JsonException::from_lint)
        .collect();

    let context_stale_exceptions: Vec<JsonException<'_>> = context_report
        .stale_exceptions
        .iter()
        .map(JsonException::from_lint)
        .collect();

    let exception_entries: Vec<JsonException<'_>> =
        exceptions.iter().map(JsonException::from_lint).collect();

    let json_packages: Vec<JsonPackage<'_>> = packages
        .iter()
        .map(|p: &lint::WorkspacePackage| JsonPackage {
            name: &p.name,
            role: p.role.as_str(),
            context: p.context.as_deref(),
        })
        .collect();

    let matrix: Vec<JsonMatrixRow> = ALL_ROLES
        .iter()
        .map(|&r: &Role| JsonMatrixRow {
            consumer_role: r.as_str(),
            allowed_deps: r.allowed_deps().iter().map(|d: &Role| d.as_str()).collect(),
        })
        .collect();

    let rules: Vec<JsonRule<'_>> = vec![
        JsonRule {
            id: "role-matrix",
            description:
                "Every workspace-internal dep edge respects the hex-arch role matrix (or is sanctioned by hex-lint-exceptions.toml).",
            status: if role_report.unsanctioned.is_empty() { "pass" } else { "fail" },
            failure_count: role_report.unsanctioned.len(),
        },
        JsonRule {
            id: "exceptions-honest",
            description:
                "Every entry in hex-lint-exceptions.toml corresponds to a real, current violation (no stale debt).",
            status: if role_report.stale_exceptions.is_empty() { "pass" } else { "fail" },
            failure_count: role_report.stale_exceptions.len(),
        },
        JsonRule {
            id: "context-isolation",
            description:
                "Every workspace-internal dep edge respects bounded-context isolation (same context, or the dependency is shared; or is sanctioned by hex-lint-exceptions.toml).",
            status: if context_report.unsanctioned.is_empty() { "pass" } else { "fail" },
            failure_count: context_report.unsanctioned.len(),
        },
    ];

    let out: JsonReport<'_> = JsonReport {
        version: 1,
        status: if had_problem { "fail" } else { "pass" },
        package_count: packages.len(),
        packages: json_packages,
        matrix,
        rules,
        violations,
        context_violations,
        exceptions: exception_entries,
        stale_exceptions,
        context_stale_exceptions,
    };

    match serde_json::to_string_pretty(&out) {
        Ok(s) => println!("{s}"),
        Err(e) => {
            // Fallback: never lose the failure signal.
            eprintln!("hex-lint: json serialization failed: {e}");
        }
    }
}

const ALL_ROLES: &[Role] = Role::ALL;

#[derive(Serialize)]
struct JsonReport<'a> {
    version: u32,
    status: &'a str,
    package_count: usize,
    packages: Vec<JsonPackage<'a>>,
    matrix: Vec<JsonMatrixRow<'a>>,
    rules: Vec<JsonRule<'a>>,
    violations: Vec<JsonViolation<'a>>,
    context_violations: Vec<JsonContextViolation<'a>>,
    exceptions: Vec<JsonException<'a>>,
    stale_exceptions: Vec<JsonException<'a>>,
    context_stale_exceptions: Vec<JsonException<'a>>,
}

#[derive(Serialize)]
struct JsonPackage<'a> {
    name: &'a str,
    role: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<&'a str>,
}

#[derive(Serialize)]
struct JsonMatrixRow<'a> {
    consumer_role: &'a str,
    allowed_deps: Vec<&'a str>,
}

#[derive(Serialize)]
struct JsonRule<'a> {
    id: &'a str,
    description: &'a str,
    status: &'a str,
    failure_count: usize,
}

#[derive(Serialize)]
struct JsonViolation<'a> {
    axis: Axis,
    consumer: &'a str,
    consumer_role: &'a str,
    dep: &'a str,
    dep_role: &'a str,
    sanctioned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    ticket: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
    remediation: JsonRemediation<'a>,
}

#[derive(Serialize)]
struct JsonContextViolation<'a> {
    axis: Axis,
    consumer: &'a str,
    consumer_context: &'a str,
    dep: &'a str,
    dep_context: &'a str,
    sanctioned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    ticket: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
    remediation: JsonRemediation<'a>,
}

#[derive(Serialize)]
struct JsonRemediation<'a> {
    rule: &'a str,
    fixes: &'a [&'a str],
}

#[derive(Serialize)]
struct JsonException<'a> {
    consumer: &'a str,
    dep: &'a str,
    axis: Axis,
    ticket: &'a str,
    reason: &'a str,
}

impl<'a> JsonException<'a> {
    fn from_lint(e: &'a Exception) -> Self {
        Self {
            consumer: &e.consumer,
            dep: &e.dep,
            axis: e.axis,
            ticket: &e.ticket,
            reason: &e.reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_args, ParseOutcome};
    use crate::role::Role;

    fn args(slice: &[&str]) -> Vec<String> {
        slice.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn empty_args_runs_with_defaults() {
        let outcome = parse_args(args(&[])).expect("empty args ok");
        let a = match outcome {
            ParseOutcome::Run(a) => a,
            _ => panic!("expected Run"),
        };
        assert!(a.exceptions.is_none());
        assert!(a.manifest_path.is_none());
    }

    #[test]
    fn space_separated_value() {
        let outcome = parse_args(args(&["--exceptions", "foo.toml"])).expect("ok");
        let a = match outcome {
            ParseOutcome::Run(a) => a,
            _ => panic!("expected Run"),
        };
        assert_eq!(a.exceptions.as_deref().unwrap().to_str(), Some("foo.toml"));
    }

    #[test]
    fn equals_separated_value() {
        let outcome = parse_args(args(&["--exceptions=foo.toml"])).expect("ok");
        let a = match outcome {
            ParseOutcome::Run(a) => a,
            _ => panic!("expected Run"),
        };
        assert_eq!(a.exceptions.as_deref().unwrap().to_str(), Some("foo.toml"));
    }

    #[test]
    fn equals_value_with_repeated_prefix_kept_intact() {
        // Regression: previous trim_start_matches stripped the prefix
        // repeatedly. strip_prefix removes it exactly once.
        let outcome = parse_args(args(&["--exceptions=--exceptions=foo"])).expect("ok");
        let a = match outcome {
            ParseOutcome::Run(a) => a,
            _ => panic!("expected Run"),
        };
        assert_eq!(
            a.exceptions.as_deref().unwrap().to_str(),
            Some("--exceptions=foo")
        );
    }

    #[test]
    fn short_e_flag() {
        let outcome = parse_args(args(&["-e", "foo.toml"])).expect("ok");
        let a = match outcome {
            ParseOutcome::Run(a) => a,
            _ => panic!("expected Run"),
        };
        assert_eq!(a.exceptions.as_deref().unwrap().to_str(), Some("foo.toml"));
    }

    #[test]
    fn manifest_path_space_and_equals() {
        for raw in [
            vec!["--manifest-path", "Cargo.toml"],
            vec!["--manifest-path=Cargo.toml"],
        ] {
            let owned: Vec<String> = raw.into_iter().map(String::from).collect();
            let outcome = parse_args(owned).expect("ok");
            let a = match outcome {
                ParseOutcome::Run(a) => a,
                _ => panic!("expected Run"),
            };
            assert_eq!(
                a.manifest_path.as_deref().unwrap().to_str(),
                Some("Cargo.toml")
            );
        }
    }

    #[test]
    fn missing_value_is_an_error() {
        assert!(parse_args(args(&["--exceptions"])).is_err());
        assert!(parse_args(args(&["--manifest-path"])).is_err());
    }

    #[test]
    fn unknown_flag_is_an_error() {
        let err = parse_args(args(&["--what"])).unwrap_err();
        assert!(err.contains("--what"), "{err}");
    }

    #[test]
    fn help_flag_short_circuits() {
        let outcome = parse_args(args(&["--help"])).expect("ok");
        assert!(matches!(outcome, ParseOutcome::HelpRequested));
        let outcome = parse_args(args(&["-h"])).expect("ok");
        assert!(matches!(outcome, ParseOutcome::HelpRequested));
    }

    #[test]
    fn version_flag_short_circuits() {
        let outcome = parse_args(args(&["--version"])).expect("ok");
        assert!(matches!(outcome, ParseOutcome::VersionRequested));
        let outcome = parse_args(args(&["-V"])).expect("ok");
        assert!(matches!(outcome, ParseOutcome::VersionRequested));
    }

    #[test]
    fn format_default_is_text() {
        let outcome = parse_args(args(&[])).expect("ok");
        let a = match outcome {
            ParseOutcome::Run(a) => a,
            _ => panic!("expected Run"),
        };
        assert_eq!(a.format.as_str(), "text");
    }

    #[test]
    fn format_json_short_and_long() {
        for raw in [
            vec!["--format", "json"],
            vec!["--format=json"],
            vec!["-f", "json"],
        ] {
            let owned: Vec<String> = raw.into_iter().map(String::from).collect();
            let outcome = parse_args(owned).expect("ok");
            let a = match outcome {
                ParseOutcome::Run(a) => a,
                _ => panic!("expected Run"),
            };
            assert_eq!(a.format.as_str(), "json");
        }
    }

    #[test]
    fn format_text_explicit_is_accepted() {
        let outcome = parse_args(args(&["--format=text"])).expect("ok");
        let a = match outcome {
            ParseOutcome::Run(a) => a,
            _ => panic!("expected Run"),
        };
        assert_eq!(a.format.as_str(), "text");
    }

    #[test]
    fn format_unknown_is_an_error() {
        let err = parse_args(args(&["--format=yaml"])).unwrap_err();
        assert!(err.contains("yaml"), "{err}");
        let err = parse_args(args(&["-f", "xml"])).unwrap_err();
        assert!(err.contains("xml"), "{err}");
    }

    #[test]
    fn format_missing_value_is_an_error() {
        assert!(parse_args(args(&["--format"])).is_err());
        assert!(parse_args(args(&["-f"])).is_err());
    }

    #[test]
    fn explain_parses_role() {
        let outcome = parse_args(args(&["explain", "usecase"])).expect("ok");
        match outcome {
            ParseOutcome::Explain(role) => assert_eq!(role, Role::Usecase),
            _ => panic!("expected Explain"),
        }
    }

    #[test]
    fn explain_unknown_role_is_an_error() {
        let err = parse_args(args(&["explain", "nonsense"])).unwrap_err();
        assert!(err.contains("nonsense"), "{err}");
    }

    #[test]
    fn explain_missing_role_is_an_error() {
        assert!(parse_args(args(&["explain"])).is_err());
    }

    #[test]
    fn help_documents_every_enforced_axis() {
        // Guards the exact drift that prompted this: `main` enforces both the
        // role matrix and context isolation, so `--help` must document both.
        let help: String = super::help_text();

        // Role axis.
        assert!(help.contains("ROLES:"), "help must list the roles");
        assert!(
            help.contains("role = \"domain\""),
            "help must show the role tag"
        );

        // Context axis — the part that was missing.
        assert!(
            help.contains("CONTEXT ISOLATION"),
            "help must document the context axis"
        );
        assert!(
            help.contains("context = "),
            "help must show the context tag"
        );
        assert!(
            help.contains("\"shared\""),
            "help must name the reserved shared context"
        );
        assert!(
            help.contains("partial adoption"),
            "help must state the all-or-nothing adoption rule"
        );

        // Exceptions cover both axes.
        assert!(
            help.contains("axis = \"context\""),
            "help must show the context exception axis key"
        );
    }
}
