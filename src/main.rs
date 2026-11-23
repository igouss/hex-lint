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
//! This file is the composition root: it hands the arguments to `args`, calls
//! the adapters, runs the use cases, and passes their reports to `help`,
//! `render_text` or `render_json`. No business logic lives here.

#![allow(clippy::print_stderr, reason = "this IS a CLI tool")]
#![allow(clippy::print_stdout, reason = "this IS a CLI tool")]

mod args;
mod capabilities;
mod context;
mod context_check;
mod exceptions;
mod help;
mod lint;
mod remediation;
mod render_json;
mod render_text;
mod role;
mod role_check;
mod rule;
#[cfg(test)]
mod tests;
mod workspace;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::args::{parse_args, Args, Format, ParseOutcome};
use crate::help::{print_explain, print_help};
use crate::render_json::render_json;
use crate::render_text::render_text;

const DEFAULT_EXCEPTIONS_FILENAME: &str = "hex-lint-exceptions.toml";

fn main() -> ExitCode {
    let args: Args = match parse_args(env::args().skip(1)) {
        Ok(ParseOutcome::Run(a)) => a,
        Ok(ParseOutcome::Explain(role)) => {
            print_explain(role);
            return ExitCode::SUCCESS;
        }
        // Answered before any workspace is loaded, on purpose: the contract is
        // a property of the tool, not of a repo, so an agent can read it from
        // anywhere — including before the workspace it is about to write exists.
        Ok(ParseOutcome::CapabilitiesRequested) => {
            println!("{}", capabilities::render(env!("CARGO_PKG_VERSION")));
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
        || !role_report.unknown_crate_exceptions.is_empty()
        || !context_report.unsanctioned.is_empty()
        || !context_report.stale_exceptions.is_empty()
        || !context_report.unknown_crate_exceptions.is_empty();

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
