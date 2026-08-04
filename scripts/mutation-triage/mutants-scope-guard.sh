#!/usr/bin/env bash
# BLOCKING: a mutation run must name its scope.
#
# `Cargo.toml` sets `default-members = ["apps/pipe-cli"]`, so an unscoped
# cargo-mutants invocation quietly enumerates ONE app instead of the tree.
# Measured on 2026-08-03: 19 mutants unscoped, 5295 with `--workspace`.
#
# That gap is not a performance footnote, it is a correctness hole. A census
# is what the equivalence ledger keys against; every key validated against a
# 19-mutant census would be validated against 0.4% of the code, and "no
# dangling entries" would mean nothing at all.
#
# Refuse rather than pick a default: guessing `--workspace` would launch a
# sweep nobody asked for (and the epic's standing rule is never to sweep the
# whole workspace in one run), while guessing the default members would
# reproduce exactly the hole this guard exists to close.
#
# Two callers, two different answers about `--workspace`, so the caller says
# which it is:
#
#   mutants-scope-guard.sh -- "$@"                   sweeping: --workspace REFUSED
#   mutants-scope-guard.sh --allow-workspace -- "$@" listing:  --workspace fine
#
# A census of the whole workspace is seconds and no build; a *sweep* of it was
# measured at 5295 mutants at --timeout=120 --jobs=2 on this machine, which is
# days of wall clock. One predicate cannot serve both, and the version that
# accepted `--workspace` for both waved through exactly the run the epic's
# standing rule forbids.

set -euo pipefail

allow_workspace=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --allow-workspace) allow_workspace=1; shift ;;
        --) shift; break ;;
        *) break ;;
    esac
done

# `-f`/`--file` is deliberately NOT accepted. It is a *filter*, not a scope:
# it narrows the selection cargo-mutants already made, and that selection is
# `default-members` unless a package scope says otherwise. Measured: a bare
# `-f crates/sandbox-protocol/src/lib.rs` selects ZERO mutants, and a run that
# selects nothing exits successfully having tested nothing -- which reads as
# "everything killed". That is a false green sitting directly on the
# kill-verification rule, so the guard refuses it.
# `-p<value>` is accepted because clap accepts it: refusing an invocation the
# tool itself honours is a false red, and a gate that cries wolf on correct
# input teaches the operator to stop believing it.
for arg in "$@"; do
    case "$arg" in
        -p|--package|-D|--in-diff)
            exit 0
            ;;
        -p=*|--package=*|-D=*|--in-diff=*|-p?*)
            exit 0
            ;;
        --workspace)
            if [[ "$allow_workspace" -eq 1 ]]; then
                exit 0
            fi
            cat >&2 <<'WS'
mutants: REFUSING a whole-workspace SWEEP.

  Measured 2026-08-03: 5295 mutants at --timeout=120 --jobs=2 on this machine.
  Per the epic's standing rule, never sweep the whole workspace in one run:
  go per-crate, per-file, or --in-diff.

  `just mutants-list --workspace` is fine -- a census is seconds and no build.
WS
            exit 1
            ;;
    esac
done

cat >&2 <<'EOF'
mutants: REFUSING to run without an explicit scope.

  Cargo.toml pins `default-members = ["apps/pipe-cli"]`, so an unscoped run
  enumerates one app, not the tree:

      unscoped      19 mutants
      --workspace 5295 mutants      (both measured 2026-08-03)

  A ledger key validated against the 19 would be validated against 0.4% of
  the code, and "no dangling entries" would mean nothing.

  Pass a scope:
      just mutants-list -p <crate>   (or --workspace: census only, no build)
      just mutants      -p <crate>
      just mutants      -p <crate> -f <path/to/file.rs>   (-f FILTERS, -p SCOPES)
      just mutants-diff                                   (scoped by --in-diff)

  `-f`/`--file` on its own is NOT a scope. It narrows a selection that is
  still `default-members`, so on any file outside them it selects ZERO
  mutants -- and a run that selects nothing exits successfully having tested
  nothing, which reads as "everything killed".

  Per the epic's standing rule, never sweep the whole workspace in one run:
  go per-crate, per-file, or --in-diff.
EOF
exit 1
