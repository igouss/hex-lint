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

mod exceptions;
mod lint;
mod role;
mod workspace;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;

use crate::lint::Exception;
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
    println!("    -f, --format <FMT>        Output format: text (default) or json.");
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

fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<ParseOutcome, String> {
    let mut it = args.into_iter();
    let mut exceptions: Option<PathBuf> = None;
    let mut manifest_path: Option<PathBuf> = None;
    let mut format: Format = Format::Text;

    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(ParseOutcome::HelpRequested),
            "-V" | "--version" => return Ok(ParseOutcome::VersionRequested),
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

    let report: lint::LintReport = lint::run(&ws.packages, &ws.edges, &exceptions);
    let had_problem: bool = !report.unsanctioned.is_empty() || !report.stale_exceptions.is_empty();

    match args.format {
        Format::Text => render_text(&ws.packages, &report, &exceptions, had_problem),
        Format::Json => render_json(&ws.packages, &report, &exceptions, had_problem),
    }

    if had_problem {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn render_text(
    packages: &[lint::WorkspacePackage],
    report: &lint::LintReport,
    exceptions: &[Exception],
    had_problem: bool,
) {
    if !report.unsanctioned.is_empty() {
        eprintln!("hex-lint: unsanctioned hex-arch role violations (not in exceptions file):");
        for v in &report.unsanctioned {
            eprintln!(
                "  {} ({}) -> {} ({}): forbidden",
                v.consumer,
                v.consumer_role.as_str(),
                v.dep,
                v.dep_role.as_str()
            );
        }
    }

    if !report.stale_exceptions.is_empty() {
        eprintln!("hex-lint: exceptions file entries that no longer match a real violation:");
        eprintln!("(remove them — debt paid off?)");
        for e in &report.stale_exceptions {
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
            report.violations.len(),
            exceptions.len()
        );
    }
}

fn render_json(
    packages: &[lint::WorkspacePackage],
    report: &lint::LintReport,
    exceptions: &[Exception],
    had_problem: bool,
) {
    let exc_by_key: std::collections::BTreeMap<(&str, &str), &Exception> = exceptions
        .iter()
        .map(|e| ((e.consumer.as_str(), e.dep.as_str()), e))
        .collect();

    let violations: Vec<JsonViolation<'_>> = report
        .violations
        .iter()
        .map(|v| {
            let exc: Option<&&Exception> = exc_by_key.get(&(v.consumer.as_str(), v.dep.as_str()));
            JsonViolation {
                consumer: &v.consumer,
                consumer_role: v.consumer_role.as_str(),
                dep: &v.dep,
                dep_role: v.dep_role.as_str(),
                sanctioned: exc.is_some(),
                ticket: exc.map(|e| e.ticket.as_str()),
                reason: exc.map(|e| e.reason.as_str()),
            }
        })
        .collect();

    let stale_exceptions: Vec<JsonException<'_>> = report
        .stale_exceptions
        .iter()
        .map(JsonException::from_lint)
        .collect();

    let exception_entries: Vec<JsonException<'_>> =
        exceptions.iter().map(JsonException::from_lint).collect();

    let json_packages: Vec<JsonPackage<'_>> = packages
        .iter()
        .map(|p| JsonPackage {
            name: &p.name,
            role: p.role.as_str(),
        })
        .collect();

    let matrix: Vec<JsonMatrixRow> = ALL_ROLES
        .iter()
        .map(|&r| JsonMatrixRow {
            consumer_role: r.as_str(),
            allowed_deps: r.allowed_deps().iter().map(|d| d.as_str()).collect(),
        })
        .collect();

    let rules: Vec<JsonRule<'_>> = vec![
        JsonRule {
            id: "role-matrix",
            description:
                "Every workspace-internal dep edge respects the hex-arch role matrix (or is sanctioned by hex-lint-exceptions.toml).",
            status: if report.unsanctioned.is_empty() { "pass" } else { "fail" },
            failure_count: report.unsanctioned.len(),
        },
        JsonRule {
            id: "exceptions-honest",
            description:
                "Every entry in hex-lint-exceptions.toml corresponds to a real, current violation (no stale debt).",
            status: if report.stale_exceptions.is_empty() { "pass" } else { "fail" },
            failure_count: report.stale_exceptions.len(),
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
        exceptions: exception_entries,
        stale_exceptions,
    };

    match serde_json::to_string_pretty(&out) {
        Ok(s) => println!("{s}"),
        Err(e) => {
            // Fallback: never lose the failure signal.
            eprintln!("hex-lint: json serialization failed: {e}");
        }
    }
}

const ALL_ROLES: &[Role] = &[
    Role::Domain,
    Role::Usecase,
    Role::PortAndAdapter,
    Role::DrivenAdapter,
    Role::DrivingAdapter,
    Role::Infra,
    Role::CompositionRoot,
];

#[derive(Serialize)]
struct JsonReport<'a> {
    version: u32,
    status: &'a str,
    package_count: usize,
    packages: Vec<JsonPackage<'a>>,
    matrix: Vec<JsonMatrixRow<'a>>,
    rules: Vec<JsonRule<'a>>,
    violations: Vec<JsonViolation<'a>>,
    exceptions: Vec<JsonException<'a>>,
    stale_exceptions: Vec<JsonException<'a>>,
}

#[derive(Serialize)]
struct JsonPackage<'a> {
    name: &'a str,
    role: &'a str,
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
    consumer: &'a str,
    consumer_role: &'a str,
    dep: &'a str,
    dep_role: &'a str,
    sanctioned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    ticket: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
}

#[derive(Serialize)]
struct JsonException<'a> {
    consumer: &'a str,
    dep: &'a str,
    ticket: &'a str,
    reason: &'a str,
}

impl<'a> JsonException<'a> {
    fn from_lint(e: &'a Exception) -> Self {
        Self {
            consumer: &e.consumer,
            dep: &e.dep,
            ticket: &e.ticket,
            reason: &e.reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_args, ParseOutcome};

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
}
