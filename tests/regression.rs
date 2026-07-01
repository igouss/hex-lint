//! Acceptance #1 — regression lock for the opt-in guarantee.
//!
//! With zero `context` tags declared anywhere, hex-lint must behave exactly as
//! it did before the context axis existed. `tests/golden/no_context.txt` was
//! captured from the pre-change binary; these tests prove the current binary
//! still reproduces it byte-for-byte and still exits clean.

mod common;

use std::path::PathBuf;
use std::process::Output;

/// Absolute path to the checked-in golden capture.
fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("no_context.txt")
}

#[test]
fn no_context_stdout_matches_the_golden_byte_for_byte() {
    let out: Output = common::run("no_context", &[]);
    let golden: Vec<u8> = std::fs::read(golden_path()).expect("read golden capture");
    assert_eq!(
        out.stdout, golden,
        "no_context stdout must equal the golden byte-for-byte"
    );
}

#[test]
fn no_context_exits_zero() {
    let out: Output = common::run("no_context", &[]);
    assert!(out.status.success(), "no_context must exit 0");
}
