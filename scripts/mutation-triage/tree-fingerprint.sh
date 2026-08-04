#!/usr/bin/env bash
# The identity of a WORKING TREE, not of a commit.
#
# `git rev-parse HEAD` does not move when you edit a file, and editing tests at
# one commit is the entire activity of the survivor triage loop. A green stamp
# keyed to the commit therefore stays valid across every failed attempt at a
# fix: add the test for a survivor, get it wrong, and the stamp still
# authorises `--baseline skip` — on a red tree, where every mutant looks
# killed and the test-gap bead closes on a fabricated kill.
#
# Three inputs, because all three change what `cargo test` compiles:
#   - the commit,
#   - every uncommitted change to a tracked file,
#   - the contents of untracked, non-ignored files (a new `tests/foo.rs` is
#     compiled by cargo without any tracked file changing at all).
#
# The sweep's OWN artefacts are excluded, and that exclusion is load-bearing.
# `mutants.out/` is untracked, so in a repo that has not gitignored it every
# sweep rewrites ~90 files and changes the fingerprint — meaning step 4 of
# `mutants-loop` invalidates the green stamp step 3 has just taken, and
# `--baseline skip` silently never applies again. Nothing in that failure is
# visible; the stamp is simply never found. Artefacts are not compiled input,
# so they do not belong in a fingerprint of what `cargo test` would build.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

{
    git rev-parse HEAD
    git diff HEAD
    git ls-files --others --exclude-standard -z \
        -- ':(exclude)mutants.out/' ':(exclude)mutants.out.old/' |
        xargs -0 -r sha256sum
} | sha256sum | cut -c1-16
