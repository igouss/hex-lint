#!/usr/bin/env bash
# BLOCKING: no generator vocabulary in the neutral core.
#
# hex-lint makes the *dependency* leak impossible — a domain or usecase crate
# cannot depend on `cargo-mutants-report`. This closes the other half: the
# *vocabulary* leak, where the core learns a particular tool's spelling of a
# mutation class, its output file names, or its command-line flags without
# any dependency edge to show for it.
#
# Why it matters: the planned operator pack has to emit into this same
# pipeline, and the ledger's promise is that a claim adjudicated under one
# generator is honoured under another. That promise holds only while the core
# genuinely does not know which tool ran.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# This gate guards THIS TOOL'S OWN source, so it runs in this repo, not in a
# consuming project: a project that installs the `mutants-triage` binary has no
# neutral core of its own to violate. Consumers wire `mutants-ledger-check`
# into their verify; this one stays here.
NEUTRAL=(
    "crates/mutation-triage-domain/src"
    "crates/mutation-triage/src"
)

# Each pattern is a piece of one tool's vocabulary that must never appear in
# the neutral core: its name, its output paths, its class field, three of its
# result words, and the two flags whose confusion this whole loop exists to
# mechanize away.
#
# `MISSED` is on the list because it was found in the core: the ledger's
# refutation rule compared a free-string verdict against that literal -- one
# tool's CONSOLE word, in the functional core, making a refutation performed
# with any other runner permanently inadmissible. Prose counts too: a comment
# is how the next reader learns the vocabulary.
PATTERNS=(
    'cargo-mutants'
    'mutants\.out'
    'outcomes\.json'
    'genre'
    'MissedMutant'
    'CaughtMutant'
    'MISSED'
    '\-\-re'
    '\-F'
)

# A missing directory is a FAILURE, not a skip. The version that skipped it
# reported "the core names no generator" after grepping zero files, so a
# rename of either crate would have silently retired the gate -- the same
# shape as a test suite that passes because it ran nothing. Precedent:
# `effect-audit --require-domain` exits 2 rather than passing green having
# audited nothing, and hex-arch.sh refuses when hex-lint is absent.
for dir in "${NEUTRAL[@]}"; do
    if [[ ! -d "$dir" ]]; then
        echo "mutant-schema-neutral: REFUSING -- neutral core directory '$dir' does not exist." >&2
        echo "mutant-schema-neutral: a gate that reports success for work it did not do is worse than no gate. Update NEUTRAL in this script after the rename." >&2
        exit 1
    fi
done

found=""
for dir in "${NEUTRAL[@]}"; do
    for pattern in "${PATTERNS[@]}"; do
        if hits=$(grep -rnE -- "$pattern" "$dir" 2>/dev/null); then
            found+="$hits"$'\n'
        fi
    done
done

if [[ -n "${found//[$'\n' ]/}" ]]; then
    echo "mutant-schema-neutral: generator vocabulary leaked into the neutral core:" >&2
    echo "$found" >&2
    cat >&2 <<'EOF'

  The domain and the use cases must not know which tool produced a mutant.
  Move the offending code into a driven adapter (cargo-mutants-report is the
  only crate allowed to name that tool) and reach it through a port.
EOF
    exit 1
fi

echo "mutant-schema-neutral: the core names no generator"
