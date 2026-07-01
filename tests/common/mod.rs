//! Shared process-level harness for the fixture integration tests.
//!
//! Both `regression.rs` and `context_isolation.rs` include this module and
//! drive the compiled `hex-lint` binary against a fixture workspace. Keeping
//! the invocation here means neither test file knows *how* the binary is run —
//! they assert only on its observable output (exit status, stdout, stderr).
//!
//! This lives under `tests/common/` (a subdirectory `mod.rs`) so Cargo treats
//! it as a shared module, not as its own integration-test binary.

use std::path::PathBuf;
use std::process::{Command, Output};

/// Run the compiled `hex-lint` against fixture `<name>` (rooted at
/// `tests/fixtures/<name>/Cargo.toml`), threading any `extra` args through
/// verbatim (e.g. `--format=json`, `--exceptions <path>`), and capture its
/// output. The fixture manifest is always resolved from `CARGO_MANIFEST_DIR`
/// so the tests are independent of the process working directory.
pub fn run(fixture: &str, extra: &[&str]) -> Output {
    let manifest: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(fixture)
        .join("Cargo.toml");
    Command::new(env!("CARGO_BIN_EXE_hex-lint"))
        .arg("--manifest-path")
        .arg(&manifest)
        .args(extra)
        .output()
        .expect("spawn hex-lint binary")
}
