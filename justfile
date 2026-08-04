# hex-lint gates.
#
# The recipes here are the same checks `.github/workflows/ci.yml` runs, so a
# green `just verify` is a green CI, and the mutation-triage ledger gate.

# ---- mutation-triage -------------------------------------------------------
# Required only because section 2 overrides an mt_* variable: without it `just`
# refuses the redefinition outright rather than preferring the later one.
set allow-duplicate-variables := true

import 'scripts/mutation-triage/mutation-triage.just'

# hex-lint is a single package, not a workspace, and it has no optional
# features. The default green command names `--workspace --all-features`, which
# resolves to the same suite here; it is spelled out so the stamp and the sweep
# are visibly the same set of tests.
mt_green_cmd := "cargo nextest run --all-features"

default: verify

# ---- the gate --------------------------------------------------------------
# `mutants-gates` checks LEDGER INTEGRITY, not mutation score: no build, under
# a second. A sweep does NOT belong here — per-mutant verdicts flake via
# timeouts on a loaded machine, so a sweep in a gate is a red that means
# nothing.

[doc("Everything CI gates on, plus the ledger-integrity check.")]
verify: fmt-check clippy mutants-gates test self-lint

fmt-check:
    cargo fmt --all --check

clippy:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo nextest run --all-features

# hex-lint is its own first consumer: the workspace declares
# `package.metadata.hex-arch`, so the tool must pass its own rules.

[doc("Run hex-lint against hex-lint. It is its own first consumer.")]
self-lint:
    cargo run --quiet --

# ---- the loop --------------------------------------------------------------
# One crate at a time, serially:
#
#   just mutants-loop hex-lint
#
# Exit 2 is the healthy first-run outcome — real survivors exist and need
# judgement. Size the problem first, without paying for a build:
#
#   just mutants-check hex-lint
[doc("Everything cheap: refuse a bad sweep, then count the mutants. No build.")]
mutants-check crate:
    just mutants-preflight {{ crate }}
    just mutants-list -p {{ crate }}
